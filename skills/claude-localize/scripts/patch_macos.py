#!/usr/bin/env python3
"""
ClaudeCN — Claude Desktop 中文(zh-CN)汉化器 (macOS)

设计目标：自适应。它在运行时读取本机 Claude.app 当前的 i18n 结构，而不是依赖
写死的路径/正则——这样 Claude Desktop 每次更新它都能继续工作，或在结构变化时
明确告诉你哪里需要人工确认。

确定性的活（备份、合并 JSON、重签名、改配置、校验）由本脚本完成；
需要"动脑"的活（翻译新增文案）由调用本脚本的 Claude 模型完成——见 SKILL.md。

子命令：
  detect    探测安装：路径、版本、i18n 布局、语言加载机制、当前覆盖率 → 打印 JSON
  prepare   只读地算出"还没翻译的新增 key"，写到 <work>/untranslated.json 供模型翻译
  apply     执行完整汉化：复制→合并译文→(必要时)注入白名单→重签名→换入(提权一次)→写配置→校验
  restore   从备份恢复英文原版（提权一次）
  status    打印当前是否已汉化

约定：
  - 译文基准文件随 skill 内置在 assets/ 下（zh-CN.base.json 等）。
  - apply 时若存在 <work>/delta-zh.json（模型翻译的新增增量），会一并合并进去。
  - 备份位置沿用 GUI 版：~/Library/Application Support/ClaudeCN/backups/
"""

import argparse
import json
import os
import plistlib
import shutil
import subprocess
import sys
import tempfile
import re
from pathlib import Path

HOME = Path.home()
APP_PATH = Path("/Applications/Claude.app")
CONFIG_DIR = HOME / "Library/Application Support/Claude"
CONFIG_FILE = CONFIG_DIR / "config.json"
# 备份放 skill 自己的目录（不再依赖 ClaudeCN GUI；纯 skill 用户也讲得通）
BACKUP_DIR = HOME / "Library/Application Support/claude-localize/backups"
BACKUP_ZIP = BACKUP_DIR / "Claude-original.zip"
BACKUP_VER = BACKUP_DIR / "backup-version.txt"
# 兼容旧 GUI(ClaudeCN) 的备份位置：恢复/查重时也会去那里找
LEGACY_BACKUP_DIR = HOME / "Library/Application Support/ClaudeCN/backups"
LOCALE = "zh-CN"  # 若实测发现新版把 zh-CN 归一成别的，改这里即可

SKILL_DIR = Path(__file__).resolve().parent.parent
ASSETS = SKILL_DIR / "assets"


# ---------------------------------------------------------------- 探测 / 适配

def find_app(explicit: str | None) -> Path:
    if explicit:
        p = Path(explicit)
        if p.exists():
            return p
    for cand in [APP_PATH, HOME / "Applications/Claude.app"]:
        if cand.exists():
            return cand
    raise SystemExit("未找到 Claude.app（默认 /Applications/Claude.app）。请用 --app 指定路径。")


def app_version(app: Path) -> str:
    info = app / "Contents/Info.plist"
    try:
        with open(info, "rb") as f:
            return plistlib.load(f).get("CFBundleShortVersionString", "")
    except Exception:
        return ""


def find_i18n_dir(app: Path) -> Path | None:
    """优先标准路径；否则在 Contents/Resources 下找包含 en-US.json 的目录（自适应）。"""
    std = app / "Contents/Resources/ion-dist/i18n"
    if (std / "en-US.json").exists():
        return std
    res = app / "Contents/Resources"
    if res.exists():
        for en in res.rglob("en-US.json"):
            # 取明显是前端 i18n 的那个（同目录通常还有别的 locale）
            sibs = list(en.parent.glob("*.json"))
            if len(sibs) >= 3:
                return en.parent
    return None


def find_assets_dir(app: Path) -> Path | None:
    """放 index-*.js 的目录。优先标准路径，否则自适应搜索。"""
    std = app / "Contents/Resources/ion-dist/assets/v1"
    if std.exists() and any(std.glob("index-*.js")):
        return std
    res = app / "Contents/Resources"
    if res.exists():
        for js in res.rglob("index-*.js"):
            return js.parent
    return None


# 语言白名单（“支持的 locale”数组）现版藏在内容哈希的 *chunk* 里（如
# assets/v1/ccc72bfa9-*.js）——不在 index-*.js。旧逻辑只扫 index-*.js 会漏掉它，
# 于是 detect 误报“无白名单”，zh-CN 永远不被登记为真正的 locale，汉化只能靠覆盖
# en-US 硬撑；一旦渲染层/主进程协商出的 locale 缺对应 i18n 文件，加载器抛错、界面
# 永远不就绪 → 白屏（且错误被 React-Query 吞掉，日志里什么都没有）。所以扫描全部 *.js。
_WL_SIGNATURE = b'["en-US"'  # 数组字面量起始，作为快速字节预筛（避免逐个解码 700+ 文件）
_WL_PATTERNS = [
    # 精确匹配当前已知的支持-locale 数组（成员/顺序固定时优先命中）
    r'(\["en-US","de-DE","fr-FR","ko-KR","ja-JP","es-419","es-ES","it-IT","hi-IN","pt-BR","id-ID"[^\]]*?)\]',
    # 通用兜底：["en-US","xx-YY",…]（至少 3 个成员，认作 locale 列表而非普通数组）
    r'(\["en-US"(?:,"[a-z]{2,3}(?:-[A-Za-z0-9]{2,4})*"){3,}?)\]',
]


def _locale_list_js(assets_dir: Path | None) -> list[Path]:
    """assets 下所有“含硬编码 locale 数组”的 JS。现版在 chunk 而非 index-*.js，故扫全部 *.js。"""
    out = []
    if not assets_dir:
        return out
    for p in sorted(assets_dir.glob("*.js")):
        try:
            if _WL_SIGNATURE not in p.read_bytes():
                continue
        except Exception:
            continue
        out.append(p)
    return out


def _first_wl_match(text: str):
    for pat in _WL_PATTERNS:
        m = re.search(pat, text)
        if m:
            return m
    return None


def detect_whitelist(assets_dir: Path | None) -> dict:
    """是否仍有“硬编码支持-locale 数组”，以及它是否已含 zh-CN。扫描全部 *.js（含 chunk）。"""
    result = {"needs_js_patch": False, "already_has_zh": False,
              "locale_list_files": [], "scanned_js": 0}
    if not assets_dir:
        return result
    result["scanned_js"] = len(list(assets_dir.glob("*.js")))
    for p in _locale_list_js(assets_dir):
        m = _first_wl_match(p.read_text(encoding="utf-8", errors="ignore"))
        if not m:
            continue
        result["locale_list_files"].append(p.name)
        if '"zh-CN"' in m.group(0):
            result["already_has_zh"] = True
        else:
            result["needs_js_patch"] = True
    return result


def load_json(path: Path) -> dict:
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def en_source(i18n_dir: Path) -> Path:
    """纯净英文源：若 en-US.json 已被我们覆盖成中文，改用旁置的 en-US.original.json。"""
    orig = i18n_dir / "en-US.original.json"
    return orig if orig.exists() else i18n_dir / "en-US.json"


def coverage(i18n_dir: Path) -> dict:
    """现有基准译文对当前 en-US.json 的覆盖率。"""
    en = load_json(en_source(i18n_dir))
    base = load_json(ASSETS / "zh-CN.base.json")
    ek, bk = set(en), set(base)
    inter = ek & bk
    return {
        "en_keys": len(ek),
        "base_keys": len(bk),
        "covered": len(inter),
        "untranslated": len(ek - bk),
        "pct": round(100 * len(inter) / len(ek), 1) if ek else 0,
    }


def cmd_detect(args):
    app = find_app(args.app)
    i18n = find_i18n_dir(app)
    assets = find_assets_dir(app)
    out = {
        "app": str(app),
        "version": app_version(app),
        "i18n_dir": str(i18n) if i18n else None,
        "assets_dir": str(assets) if assets else None,
        "has_dynamic_dir": bool(i18n and (i18n / "dynamic").exists()),
        "whitelist": detect_whitelist(assets),
        "patched": is_patched(app, i18n),
        "has_backup": existing_backup()[0] is not None,
    }
    if i18n:
        out["coverage"] = coverage(i18n)
    print(json.dumps(out, ensure_ascii=False, indent=2))


# ------------------------------------------------------------------- 合并译文

def deep_merge(base: dict, overlay: dict) -> dict:
    """overlay 覆盖 base；都为 dict 时递归。Claude 现版是扁平表，递归只是为了兼容旧版。"""
    out = dict(base)
    for k, v in overlay.items():
        if isinstance(out.get(k), dict) and isinstance(v, dict):
            out[k] = deep_merge(out[k], v)
        else:
            out[k] = v
    return out


def build_merged(i18n_dir: Path, delta: dict | None) -> tuple[dict, dict]:
    """
    返回 (merged, untranslated)。
    merged = en-US 为底（保证未译 key 回退英文），叠加内置精校 base，再叠加本次 delta。
    untranslated = 当前 en-US 里既不在 base 也不在 delta 的 key（{key: english}）。
    """
    en = load_json(en_source(i18n_dir))
    base = load_json(ASSETS / "zh-CN.base.json")
    delta = delta or {}
    merged = deep_merge(en, base)
    merged = deep_merge(merged, delta)
    covered = set(base) | set(delta)
    untranslated = {k: v for k, v in en.items()
                    if k not in covered and isinstance(v, str)}
    return merged, untranslated


def cmd_prepare(args):
    app = find_app(args.app)
    i18n = find_i18n_dir(app)
    if not i18n:
        raise SystemExit("未找到 i18n 目录，无法 prepare。")
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    _, untranslated = build_merged(i18n, None)
    (work / "untranslated.json").write_text(
        json.dumps(untranslated, ensure_ascii=False, indent=2), encoding="utf-8")
    cov = coverage(i18n)
    print(json.dumps({
        "version": app_version(app),
        "untranslated_file": str(work / "untranslated.json"),
        "untranslated_count": len(untranslated),
        "coverage_now": cov,
        "next": "让模型翻译 untranslated.json → 写成 <work>/delta-zh.json（同样的 key），再跑 apply",
    }, ensure_ascii=False, indent=2))


# ------------------------------------------------------------------- 写入文件

# 实际生效的 locale 由账号/服务端决定（登录英文账号时常是 en-US），且 app 会把它回写进
# config.json，所以光设 config.locale=zh-CN 无效。最稳妥：把中文译文直接写进 app 真正会
# 加载的 locale 文件——尤其 en-US.json。未译 key 已在 merged 里回退英文，所以覆盖 en-US.json
# 不会丢英文兜底。原始英文存到 en-US.original.json，供重复打补丁/合并时作纯净英文源。
# en-US 必须覆盖（zh-CN 未被登记为 locale 时，app 会把中文一路协商回退到 en-US）。
# zh-* 别名覆盖渲染层(DW/navigator)或主进程(x3i)可能协商出的所有写法，确保 i18n
# 加载器永远 fetch 得到合法中文文件，绝不命中 404 → 杜绝“永不就绪”的白屏。
ZH_LOCALES = ("zh-CN", "zh", "zh-Hans", "zh-Hans-CN", "zh-Hant", "zh-TW", "zh-HK", "zh-MO", "zh-SG")
FRONTEND_LOCALES = ("en-US",) + ZH_LOCALES
TARGET_LOCALES = FRONTEND_LOCALES  # 兼容旧引用


def write_frontend(i18n_dir: Path, merged: dict):
    orig = i18n_dir / "en-US.original.json"
    if not orig.exists():
        shutil.copyfile(i18n_dir / "en-US.json", orig)
    blob = json.dumps(merged, ensure_ascii=False, sort_keys=True)
    dyn = i18n_dir / "dynamic"
    has_dyn = dyn.exists() and (dyn / "en-US.json").exists()
    for name in FRONTEND_LOCALES:
        (i18n_dir / f"{name}.json").write_text(blob, encoding="utf-8")
        if name == "en-US":
            # en-US：加载器不请求 overrides；dynamic/en-US.json 是中文回退源，二者都别动
            continue
        # overrides 会被加载器叠加在 public 之上：残留的他语 overrides 会把外语/英文
        # 顶回我们的中文，所以每个中文 locale 的 overrides 一律清成空（也避免 404）。
        (i18n_dir / f"{name}.overrides.json").write_text("{}", encoding="utf-8")
        # 加载器强制要求 dynamic/{locale}.json（!ok 直接抛错），镜像 en-US 的英文回退。
        if has_dyn:
            shutil.copyfile(dyn / "en-US.json", dyn / f"{name}.json")


def verify_frontend(i18n_dir: Path) -> list:
    """换入前自检：渲染层 i18n 加载器会 fetch 的每个文件都在且是合法 JSON。
    返回问题列表（空=OK）。把潜在的“换入后白屏”变成换入前的干净中止。"""
    problems = []
    dyn = i18n_dir / "dynamic"
    has_dyn = dyn.exists() and (dyn / "en-US.json").exists()
    for name in FRONTEND_LOCALES:
        targets = [i18n_dir / f"{name}.json"]
        if has_dyn:
            targets.append(dyn / f"{name}.json")
        ov = i18n_dir / f"{name}.overrides.json"
        if ov.exists():
            targets.append(ov)
        for f in targets:
            if not f.exists():
                problems.append(f"缺失 {f.name}")
                continue
            try:
                json.loads(f.read_text(encoding="utf-8"))
            except Exception as e:
                problems.append(f"非法 JSON {f.name}: {e}")
    return problems


def write_desktop(app: Path):
    res = app / "Contents/Resources"
    src = ASSETS / "desktop-zh-CN.base.json"
    if src.exists():
        shutil.copyfile(src, res / f"{LOCALE}.json")
    strings = ASSETS / "claude-Localizable.strings"
    if strings.exists():
        for folder in ("zh-CN.lproj", "zh_CN.lproj"):
            d = res / folder
            d.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(strings, d / "Localizable.strings")


def write_statsig(i18n_dir: Path):
    sdir = i18n_dir / "statsig"
    if not sdir.exists():
        return
    src = ASSETS / "statsig-zh-CN.base.json"
    target = sdir / f"{LOCALE}.json"
    if src.exists():
        shutil.copyfile(src, target)
    elif (sdir / "en-US.json").exists():
        shutil.copyfile(sdir / "en-US.json", target)


def patch_whitelist(assets_dir: Path) -> str:
    """把 "zh-CN" 注入“支持-locale 数组”——不管它在 index 还是 chunk 里（现版在 chunk）。
    让渲染层/主进程把 zh-CN 当一等 locale（协商会返回 zh-CN、语言可选），而不是只靠
    覆盖 en-US 硬撑。幂等：数组已含 zh-CN 则跳过。返回 injected / already / skipped / no-assets。"""
    if not assets_dir:
        return "no-assets"
    cands = _locale_list_js(assets_dir)
    if not cands:
        return "skipped"  # 这个版本没有硬编码白名单 → locale 协商是开放的，无需改 JS
    result = "skipped"
    for p in cands:
        text = p.read_text(encoding="utf-8", errors="ignore")
        m = _first_wl_match(text)
        if not m:
            continue
        if '"zh-CN"' in m.group(0):
            if result == "skipped":
                result = "already"
            continue
        # 切片注入（不用 re.sub 的反向引用，避免被捕获内容里的特殊字符干扰）
        new = text[:m.start()] + m.group(1) + ',"zh-CN"]' + text[m.end():]
        p.write_text(new, encoding="utf-8")
        result = "injected"
    return result


# ------------------------------------------------------------------- 重签名

def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def extract_entitlements(binary: Path) -> dict:
    r = run(["codesign", "-d", "--entitlements", "-", "--xml", str(binary)])
    if r.returncode != 0 or not r.stdout.strip():
        return {}
    try:
        return plistlib.loads(r.stdout.encode())
    except Exception:
        return {}


# 重签名策略：ad-hoc + hardened runtime，并从原 entitlements 里剥掉"绑定 Team/Apple
# 授权"的项。原因：camera/mic/location/virtualization 这类 com.apple.security.* 在
# ad-hoc 下可正常使用，但 keychain-access-groups、application-identifier、
# com.apple.developer.* 绑了 Team ID / provisioning，用 ad-hoc 重签却保留它们，新版
# macOS 的 AMFI 会在启动时拒绝 spawn（open 报 error 163 "Launchd job spawn failed"）。
# 实测：剥掉这几项、其余原样保留 + 补上 cs.* relaxations，app 正常启动且保住摄像头/
# 麦克风/截屏等能力。用"提取后过滤"而非写死白名单，是为了 Claude 以后新增权限也能保住。
_STRIP_KEYS = {"com.apple.application-identifier", "keychain-access-groups",
               "com.apple.developer.team-identifier"}
_STRIP_PREFIXES = ("com.apple.developer.",)
_REQUIRED_CS = {
    "com.apple.security.cs.allow-jit": True,
    "com.apple.security.cs.allow-unsigned-executable-memory": True,
    "com.apple.security.cs.disable-library-validation": True,
    "com.apple.security.cs.allow-dyld-environment-variables": True,
}


def filter_entitlements(ent: dict) -> dict:
    out = {k: v for k, v in ent.items()
           if k not in _STRIP_KEYS and not any(k.startswith(p) for p in _STRIP_PREFIXES)}
    out.update(_REQUIRED_CS)
    return out


def resign(app: Path):
    ent = filter_entitlements(extract_entitlements(app / "Contents/MacOS/Claude"))
    # ElectronTeamID 与 ad-hoc 的 TeamIdentifier 对齐
    info = app / "Contents/Info.plist"
    try:
        with open(info, "rb") as f:
            pl = plistlib.load(f)
        pl["ElectronTeamID"] = "not set"
        with open(info, "wb") as f:
            plistlib.dump(pl, f)
    except Exception:
        pass
    tmp = Path(tempfile.mkdtemp(prefix="claudecn-ent-"))
    entp = tmp / "ent.plist"
    with open(entp, "wb") as f:
        plistlib.dump(ent, f)
    run(["codesign", "--remove-signature", "--deep", str(app)])
    r = run(["codesign", "--force", "--deep", "--options", "runtime",
             "--entitlements", str(entp), "--sign", "-", str(app)])
    shutil.rmtree(tmp, ignore_errors=True)
    if r.returncode != 0:
        raise SystemExit(f"重签名失败: {r.stderr}")


# ------------------------------------------------------------------- 提权换入

def osascript_admin(shell_cmd: str):
    esc = shell_cmd.replace("\\", "\\\\").replace('"', '\\"')
    script = f'do shell script "{esc}" with administrator privileges'
    r = run(["osascript", "-e", script])
    if r.returncode != 0:
        raise SystemExit(f"提权命令失败: {r.stderr.strip()}")


BACKUP_README = (
    "这是 Claude 汉化工具 (claude-localize) 的原版备份，用于一键恢复英文原版。\n"
    "请勿删除本目录。\n\n"
    "即使误删也不要紧：Claude.app 可从 https://claude.ai/download 免费重装即恢复官方原版；\n"
    "你的聊天记录和登录都在 ~/Library/Application Support/Claude/（与 app 分开存），重装不丢。\n"
)


def existing_backup() -> tuple[Path | None, str]:
    """在新位置和旧 GUI 位置都找原版备份，返回 (zip 路径, 版本)。"""
    for d in (BACKUP_DIR, LEGACY_BACKUP_DIR):
        z = d / "Claude-original.zip"
        if z.exists():
            vf = d / "backup-version.txt"
            return z, (vf.read_text().strip() if vf.exists() else "")
    return None, ""


def backup(app: Path):
    ver = app_version(app)
    z, bver = existing_backup()
    if z and bver == ver:
        return  # 任一位置已有当前版本备份，复用，不重复占空间
    BACKUP_DIR.mkdir(parents=True, exist_ok=True)
    r = run(["ditto", "-ck", "--keepParent", str(app), str(BACKUP_ZIP)])
    if r.returncode != 0:
        raise SystemExit(f"备份失败: {r.stderr}")
    BACKUP_VER.write_text(ver)
    try:
        (BACKUP_DIR / "请勿删除_README.txt").write_text(BACKUP_README, encoding="utf-8")
    except Exception:
        pass


def claude_config_dirs() -> list[Path]:
    """所有 Claude 桌面端数据目录。关键坑：除标准 'Claude' 外，接入第三方/自定义模型时
    用的是 'Claude-3p'——活动配置可能只在这里。只写标准目录会导致 locale 设了却不生效
    （UI 仍英文）。排除我们自己的 'ClaudeCN' 备份目录。"""
    base = HOME / "Library/Application Support"
    dirs = [d for d in sorted(base.glob("Claude*"))
            if d.is_dir() and (d.name == "Claude" or d.name.startswith("Claude-"))]
    return dirs or [CONFIG_DIR]


def write_locale_config():
    for d in claude_config_dirs():
        d.mkdir(parents=True, exist_ok=True)
        cf = d / "config.json"
        cfg = {}
        if cf.exists():
            try:
                cfg = json.loads(cf.read_text())
            except Exception:
                cfg = {}
        cfg["locale"] = LOCALE
        cf.write_text(json.dumps(cfg, ensure_ascii=False, indent=2, sort_keys=True))
    # AppleLanguages 用真实 bundle id（com.anthropic.claudefordesktop）
    run(["defaults", "write", "com.anthropic.claudefordesktop", "AppleLanguages",
         "-array", "zh-CN", "zh-Hans", "en"])


def remove_locale_config():
    for d in claude_config_dirs():
        cf = d / "config.json"
        if not cf.exists():
            continue
        try:
            cfg = json.loads(cf.read_text())
            cfg.pop("locale", None)
            cf.write_text(json.dumps(cfg, ensure_ascii=False, indent=2, sort_keys=True))
        except Exception:
            pass
    run(["defaults", "delete", "com.anthropic.claudefordesktop", "AppleLanguages"])


def is_patched(app: Path, i18n: Path | None) -> bool:
    # en-US.original.json 是覆盖 en-US 时留下的标记，比 config.locale 可靠
    # （config.locale 会被 app 按账号语言回写）
    if not i18n:
        return False
    return (i18n / "en-US.original.json").exists()


def quit_claude():
    run(["osascript", "-e", 'tell application "Claude" to quit'])


def launch_claude():
    run(["open", str(APP_PATH)])


def cmd_apply(args):
    app = find_app(args.app)
    i18n_src = find_i18n_dir(app)
    if not i18n_src:
        raise SystemExit("未找到 i18n 目录，无法 apply。")
    rel_i18n = i18n_src.relative_to(app)
    assets_src = find_assets_dir(app)
    rel_assets = assets_src.relative_to(app) if assets_src else None

    delta = None
    delta_path = Path(args.work) / "delta-zh.json" if args.work else None
    if delta_path and delta_path.exists():
        delta = load_json(delta_path)

    print("· 备份原版…")
    backup(app)

    print("· 复制到临时目录…")
    tmp = Path(tempfile.mkdtemp(prefix="claudecn-patch-"))
    staged = tmp / "Claude.app"
    r = run(["cp", "-R", str(app), str(staged)])
    if r.returncode != 0:
        raise SystemExit(f"复制失败: {r.stderr}")

    i18n = staged / rel_i18n
    assets = staged / rel_assets if rel_assets else None

    print("· 合并译文…")
    merged, untrans = build_merged(i18n, delta)
    write_frontend(i18n, merged)
    write_desktop(staged)
    write_statsig(i18n)

    print("· 处理语言白名单（扫 chunk，非仅 index）…")
    wl = patch_whitelist(assets) if assets else "no-assets"
    print(f"  白名单: {wl}")

    print("· 自检渲染层 i18n（拦截会导致白屏的损坏）…")
    problems = verify_frontend(i18n)
    if problems:
        shutil.rmtree(tmp, ignore_errors=True)
        raise SystemExit(
            "汉化文件自检未通过，已中止——你的 Claude 未被改动：\n  - "
            + "\n  - ".join(problems)
            + "\n这道关专门拦截换入后会白屏的损坏。请重试；若反复失败，可能是该 Claude "
              "版本的 i18n 结构变了，去读 references/mechanism.md 自适应排查。")
    print(f"  自检通过：{len(FRONTEND_LOCALES)} 个 locale 的前端 i18n 齐全且合法")

    print("· 重签名…")
    resign(staged)
    run(["xattr", "-dr", "com.apple.quarantine", str(staged)])

    print("· 关闭 Claude 并换入（需要管理员密码）…")
    quit_claude()
    old = f"/private/tmp/ClaudeCN-old-{os.getpid()}"
    osascript_admin(f"mv '{app}' '{old}' && mv '{staged}' '{app}' && rm -rf '{old}'")
    shutil.rmtree(tmp, ignore_errors=True)

    print("· 写语言配置…")
    write_locale_config()

    # 校验
    i18n_final = find_i18n_dir(app)
    ok = is_patched(app, i18n_final)
    print("· 重启 Claude…")
    launch_claude()

    en_total = len(set(load_json(en_source(i18n_final))))
    cov_after = round(100 * (en_total - len(untrans)) / max(1, en_total), 1)
    print(json.dumps({
        "ok": ok,
        "version": app_version(app),
        "whitelist": wl,
        "frontend_locales": len(FRONTEND_LOCALES),
        "untranslated_remaining": len(untrans),
        "coverage_pct": cov_after,
    }, ensure_ascii=False, indent=2))


def cmd_restore(args):
    app = find_app(getattr(args, "app", None))
    backup_zip, backup_ver = existing_backup()
    cur_ver = app_version(app)
    if not backup_zip:
        # 备份不在了（被删/没建过）。不硬失败，给出明确出路。
        raise SystemExit(
            "没找到原版备份——但你不会被卡住：\n"
            "· Claude.app 可从 https://claude.ai/download 免费重装，即恢复官方英文原版；\n"
            "· 聊天记录和登录都在 ~/Library/Application Support/Claude/（与 app 分开存），重装不丢。\n"
            "想要英文就重装一下 Claude；想保留中文则什么都不用做。")
    # 防降级保护：当前 app 已不是本工具汉化状态、且备份更旧（多半 Claude 自己更新过）→ 中止
    if (not is_patched(app, find_i18n_dir(app)) and not getattr(args, "force", False)
            and backup_ver and backup_ver != cur_ver):
        raise SystemExit(
            f"已中止：当前 Claude（{cur_ver}）看起来不是本工具汉化的状态，而备份是旧版本"
            f"（{backup_ver}）——恢复会把 Claude 降级到 {backup_ver}。\n"
            f"· 只想要英文？现在很可能已是英文原版，无需恢复。\n"
            f"· 确实要回到备份那个版本：restore --force。")
    quit_claude()
    tmp = Path(tempfile.mkdtemp(prefix="claudecn-restore-"))
    r = run(["ditto", "-xk", str(backup_zip), str(tmp)])
    if r.returncode != 0:
        raise SystemExit(f"解压备份失败: {r.stderr}")
    restored = tmp / "Claude.app"
    if not restored.exists():
        # ditto --keepParent 会多一层
        cands = list(tmp.glob("**/Claude.app"))
        if not cands:
            raise SystemExit("备份内容异常，找不到 Claude.app")
        restored = cands[0]
    old = f"/private/tmp/ClaudeCN-old-{os.getpid()}"
    osascript_admin(f"mv '{APP_PATH}' '{old}' && mv '{restored}' '{APP_PATH}' && rm -rf '{old}'")
    shutil.rmtree(tmp, ignore_errors=True)
    remove_locale_config()
    launch_claude()
    print(json.dumps({"ok": True, "restored": str(APP_PATH)}, ensure_ascii=False))


def cmd_status(args):
    app = find_app(args.app)
    i18n = find_i18n_dir(app)
    print(json.dumps({
        "patched": is_patched(app, i18n),
        "version": app_version(app),
        "has_backup": existing_backup()[0] is not None,
    }, ensure_ascii=False, indent=2))


def main():
    ap = argparse.ArgumentParser(description="Claude Desktop 中文汉化器 (macOS)")
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("detect", "prepare", "apply", "restore", "status"):
        sp = sub.add_parser(name)
        sp.add_argument("--app", help="Claude.app 路径（默认自动探测）")
        if name in ("prepare", "apply"):
            sp.add_argument("--work", default=str(SKILL_DIR / ".work"),
                            help="工作目录（放 untranslated.json / delta-zh.json）")
        if name == "restore":
            sp.add_argument("--force", action="store_true",
                            help="强制恢复，即使会降级到备份版本")
    args = ap.parse_args()
    {"detect": cmd_detect, "prepare": cmd_prepare, "apply": cmd_apply,
     "restore": cmd_restore, "status": cmd_status}[args.cmd](args)


if __name__ == "__main__":
    main()
