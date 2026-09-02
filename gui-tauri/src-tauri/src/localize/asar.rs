//! app.asar 读写。解析 Electron asar 的 pickle 头，按路径读取 / 替换 / 删除条目，
//! 然后整体重建归档：重排偏移、重算被改文件的完整性字段（整文件 sha256 + 每 4 MB
//! 分块 sha256），并给出新头部 JSON 的 sha256，供 Info.plist 的
//! `ElectronAsarIntegrity` 使用。不依赖 tauri，纯 std + serde_json + sha2。
//!
//! 文件布局（与 @electron/asar 一致）：
//! ```text
//! [u32 = 4][u32 headerSize]                      ← size pickle，8 字节
//! [u32 payloadSize][u32 strLen][JSON…][pad→4]    ← header pickle，共 headerSize 字节
//! [文件体…]                                      ← 从 8 + headerSize 开始，按 offset 定位
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Electron 完整性分块大小（4 MB）。
pub const BLOCK_SIZE: usize = 4 * 1024 * 1024;

/// 一份已加载到内存的 asar，外加尚未落盘的替换 / 删除操作。
pub struct Asar {
    header: Value,
    body_base: usize,
    data: Vec<u8>,
    replaced: BTreeMap<String, Vec<u8>>,
    removed: BTreeSet<String>,
}

/// 条目摘要。`path` 以 `/` 开头、以 `/` 分隔，例如 `/.vite/build/index.js`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    pub path: String,
    pub size: u64,
    /// 打包在 asar 体内的文件才有；unpacked / 符号链接为 None。
    pub offset: Option<u64>,
    pub unpacked: bool,
    pub link: Option<String>,
}

impl EntryInfo {
    /// 是否是打包在 asar 体内、可以读写内容的普通文件。
    pub fn is_packed(&self) -> bool {
        self.offset.is_some() && !self.unpacked && self.link.is_none()
    }
}

/// `save_to` 的结果。
#[derive(Debug, Clone)]
pub struct SaveReport {
    /// 新头部 JSON 字符串的 sha256（hex）。写进 Info.plist 的 `ElectronAsarIntegrity`。
    pub header_sha256: String,
    #[allow(dead_code)]
    pub packed_files: usize,
    /// 写出的总字节数（含头部），供日志。
    #[allow(dead_code)]
    pub total_bytes: u64,
}

fn u32le(b: &[u8], at: usize) -> Result<u32, String> {
    let s = b
        .get(at..at + 4)
        .ok_or_else(|| format!("asar 头部截断（偏移 {at}）"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// sha256 → 小写 hex。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 按 Electron 规则生成一个文件的 `integrity` 字段。
pub fn integrity_value(content: &[u8]) -> Value {
    let mut blocks = Vec::with_capacity(content.len() / BLOCK_SIZE + 1);
    if content.is_empty() {
        blocks.push(Value::String(sha256_hex(content)));
    } else {
        for chunk in content.chunks(BLOCK_SIZE) {
            blocks.push(Value::String(sha256_hex(chunk)));
        }
    }
    let mut m = Map::new();
    m.insert("algorithm".into(), Value::String("SHA256".into()));
    m.insert("hash".into(), Value::String(sha256_hex(content)));
    m.insert("blockSize".into(), Value::from(BLOCK_SIZE as u64));
    m.insert("blocks".into(), Value::Array(blocks));
    Value::Object(m)
}

/// 把头部 JSON 编成 size pickle + header pickle（含 4 字节对齐填充）。
pub fn encode_header(json: &str) -> Vec<u8> {
    let s = json.as_bytes();
    let pad = (4 - (s.len() % 4)) % 4;
    let payload_size = 4 + s.len() + pad;
    let header_size = 4 + payload_size;
    let mut out = Vec::with_capacity(8 + header_size);
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&(header_size as u32).to_le_bytes());
    out.extend_from_slice(&(payload_size as u32).to_le_bytes());
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s);
    out.extend(std::iter::repeat_n(0u8, pad));
    out
}

fn split_path(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

fn node_at<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for part in split_path(path) {
        cur = cur.get("files")?.get(part)?;
    }
    Some(cur)
}

fn entry_info(path: &str, node: &Value) -> EntryInfo {
    EntryInfo {
        path: path.to_string(),
        size: node.get("size").and_then(Value::as_u64).unwrap_or(0),
        offset: node
            .get("offset")
            .and_then(|o| match o {
                Value::String(s) => s.parse::<u64>().ok(),
                Value::Number(n) => n.as_u64(),
                _ => None,
            }),
        unpacked: node.get("unpacked").and_then(Value::as_bool).unwrap_or(false),
        link: node.get("link").and_then(Value::as_str).map(str::to_string),
    }
}

fn walk<'a>(node: &'a Value, prefix: &str, out: &mut Vec<(String, &'a Value)>) {
    let Some(Value::Object(files)) = node.get("files") else {
        return;
    };
    for (name, child) in files {
        let path = format!("{prefix}/{name}");
        if child.get("files").is_some() {
            walk(child, &path, out);
        } else {
            out.push((path, child));
        }
    }
}

impl Asar {
    /// 读入整个 asar 并解析头部。
    pub fn open(path: &Path) -> Result<Asar, String> {
        let data = std::fs::read(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
        Asar::from_bytes(data)
    }

    /// 从内存字节解析。
    pub fn from_bytes(data: Vec<u8>) -> Result<Asar, String> {
        if data.len() < 16 {
            return Err("不是 asar：文件不足 16 字节".into());
        }
        let size_payload = u32le(&data, 0)?;
        if size_payload != 4 {
            return Err(format!("不是 asar：size pickle 载荷长度应为 4，实际 {size_payload}"));
        }
        let header_size = u32le(&data, 4)? as usize;
        let payload_size = u32le(&data, 8)? as usize;
        let str_len = u32le(&data, 12)? as usize;
        let body_base = 8 + header_size;
        if payload_size + 4 != header_size || 4 + str_len > payload_size || body_base > data.len() {
            return Err(format!(
                "asar 头部长度不自洽：headerSize={header_size} payloadSize={payload_size} strLen={str_len} total={}",
                data.len()
            ));
        }
        let header: Value = serde_json::from_slice(&data[16..16 + str_len])
            .map_err(|e| format!("asar 头部 JSON 解析失败: {e}"))?;
        if header.get("files").and_then(Value::as_object).is_none() {
            return Err("asar 头部缺少 files 对象".into());
        }
        Ok(Asar {
            header,
            body_base,
            data,
            replaced: BTreeMap::new(),
            removed: BTreeSet::new(),
        })
    }

    /// 文件体起始偏移（8 + headerSize）。
    #[allow(dead_code)]
    pub fn body_base(&self) -> usize {
        self.body_base
    }

    /// 原始头部 JSON（未应用替换 / 删除）。
    #[allow(dead_code)]
    pub fn header(&self) -> &Value {
        &self.header
    }

    /// 全部叶子条目（文件、unpacked 文件、符号链接），已排除本次标记删除的。
    pub fn entries(&self) -> Vec<EntryInfo> {
        let mut leaves = Vec::new();
        walk(&self.header, "", &mut leaves);
        leaves
            .into_iter()
            .filter(|(p, _)| !self.removed.contains(p))
            .map(|(p, n)| {
                let mut info = entry_info(&p, n);
                if let Some(r) = self.replaced.get(&p) {
                    info.size = r.len() as u64;
                }
                info
            })
            .collect()
    }

    /// 单个条目信息。
    #[allow(dead_code)]
    pub fn entry(&self, path: &str) -> Option<EntryInfo> {
        let norm = normalize(path);
        if self.removed.contains(&norm) {
            return None;
        }
        node_at(&self.header, &norm).map(|n| {
            let mut info = entry_info(&norm, n);
            if let Some(r) = self.replaced.get(&norm) {
                info.size = r.len() as u64;
            }
            info
        })
    }

    /// 读取打包文件内容（若已 `replace` 则返回替换后的内容）。
    pub fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let norm = normalize(path);
        if self.removed.contains(&norm) {
            return Err(format!("{norm} 已被标记删除"));
        }
        if let Some(r) = self.replaced.get(&norm) {
            return Ok(r.clone());
        }
        let node = node_at(&self.header, &norm).ok_or_else(|| format!("asar 里没有 {norm}"))?;
        let info = entry_info(&norm, node);
        if !info.is_packed() {
            return Err(format!("{norm} 不是打包文件（unpacked 或符号链接）"));
        }
        let off = self.body_base + info.offset.unwrap() as usize;
        let end = off + info.size as usize;
        self.data
            .get(off..end)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| format!("{norm} 的偏移越界：{off}..{end} > {}", self.data.len()))
    }

    /// 暂存替换内容；`save_to` 时落盘。只允许替换已存在的打包文件。
    pub fn replace(&mut self, path: &str, content: Vec<u8>) -> Result<(), String> {
        let norm = normalize(path);
        let node = node_at(&self.header, &norm).ok_or_else(|| format!("asar 里没有 {norm}"))?;
        if !entry_info(&norm, node).is_packed() {
            return Err(format!("{norm} 不是打包文件，不能替换"));
        }
        self.removed.remove(&norm);
        self.replaced.insert(norm, content);
        Ok(())
    }

    /// 暂存删除。返回是否存在该条目。
    pub fn remove(&mut self, path: &str) -> Result<bool, String> {
        let norm = normalize(path);
        if node_at(&self.header, &norm).is_none() {
            return Ok(false);
        }
        if node_at(&self.header, &norm).and_then(|n| n.get("files")).is_some() {
            return Err(format!("{norm} 是目录，不支持删除"));
        }
        self.replaced.remove(&norm);
        self.removed.insert(norm);
        Ok(true)
    }

    /// 是否有未落盘的修改。
    #[allow(dead_code)]
    pub fn is_dirty(&self) -> bool {
        !self.replaced.is_empty() || !self.removed.is_empty()
    }

    /// 生成新头部 JSON 与写入计划（不落盘）。
    fn plan(&self) -> Result<(String, Vec<(String, Source)>), String> {
        let mut header = self.header.clone();
        for p in &self.removed {
            remove_node(&mut header, p)?;
        }
        let mut plan = Vec::new();
        let mut offset: u64 = 0;
        relayout(&mut header, "", &mut offset, &self.replaced, &mut plan)?;
        let json = serde_json::to_string(&header).map_err(|e| format!("序列化头部失败: {e}"))?;
        Ok((json, plan))
    }

    /// 重建并写到 `out`（可与源文件不同路径；写完再由调用方决定是否覆盖原文件）。
    pub fn save_to(&self, out: &Path) -> Result<SaveReport, String> {
        let (json, plan) = self.plan()?;
        let header_bytes = encode_header(&json);
        let file = std::fs::File::create(out).map_err(|e| format!("创建 {} 失败: {e}", out.display()))?;
        let mut w = std::io::BufWriter::with_capacity(1 << 20, file);
        w.write_all(&header_bytes).map_err(|e| format!("写头部失败: {e}"))?;
        let mut total = header_bytes.len() as u64;
        for (path, src) in &plan {
            let bytes: &[u8] = match src {
                Source::Original { offset, size } => {
                    let start = self.body_base + *offset as usize;
                    let end = start + *size as usize;
                    self.data
                        .get(start..end)
                        .ok_or_else(|| format!("{path} 的原始偏移越界"))?
                }
                Source::Replaced => self
                    .replaced
                    .get(path)
                    .ok_or_else(|| format!("{path} 缺少替换内容"))?,
            };
            w.write_all(bytes).map_err(|e| format!("写 {path} 失败: {e}"))?;
            total += bytes.len() as u64;
        }
        w.flush().map_err(|e| format!("刷盘失败: {e}"))?;
        Ok(SaveReport {
            header_sha256: sha256_hex(json.as_bytes()),
            packed_files: plan.len(),
            total_bytes: total,
        })
    }

    /// 重建后的头部 JSON 的 sha256（不落盘）。与 `save_to` 返回值一致。
    #[allow(dead_code)]
    pub fn planned_header_sha256(&self) -> Result<String, String> {
        Ok(sha256_hex(self.plan()?.0.as_bytes()))
    }
}

enum Source {
    Original { offset: u64, size: u64 },
    Replaced,
}

fn normalize(path: &str) -> String {
    let parts = split_path(path);
    format!("/{}", parts.join("/"))
}

fn remove_node(root: &mut Value, path: &str) -> Result<(), String> {
    let parts = split_path(path);
    let (last, dirs) = parts.split_last().ok_or("空路径")?;
    let mut cur = root;
    for d in dirs {
        cur = cur
            .get_mut("files")
            .and_then(|f| f.get_mut(*d))
            .ok_or_else(|| format!("删除 {path} 失败：目录不存在"))?;
    }
    cur.get_mut("files")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("删除 {path} 失败：父节点不是目录"))?
        .remove(*last);
    Ok(())
}

/// 深度优先重排：给每个打包文件分配新偏移，被替换的同时更新 size 与 integrity。
fn relayout(
    node: &mut Value,
    prefix: &str,
    offset: &mut u64,
    replaced: &BTreeMap<String, Vec<u8>>,
    plan: &mut Vec<(String, Source)>,
) -> Result<(), String> {
    let Some(files) = node.get_mut("files").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    for (name, child) in files.iter_mut() {
        let path = format!("{prefix}/{name}");
        if child.get("files").is_some() {
            relayout(child, &path, offset, replaced, plan)?;
            continue;
        }
        let info = entry_info(&path, child);
        if !info.is_packed() {
            continue;
        }
        let obj = child.as_object_mut().ok_or_else(|| format!("{path} 不是对象"))?;
        if let Some(content) = replaced.get(&path) {
            obj.insert("size".into(), Value::from(content.len() as u64));
            obj.insert("integrity".into(), integrity_value(content));
            obj.insert("offset".into(), Value::String(offset.to_string()));
            plan.push((path, Source::Replaced));
            *offset += content.len() as u64;
        } else {
            let (o, s) = (info.offset.unwrap(), info.size);
            obj.insert("offset".into(), Value::String(offset.to_string()));
            plan.push((path, Source::Original { offset: o, size: s }));
            *offset += s;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用本模块自己的编码器手搓一个小 asar：两个打包文件、一个 unpacked、一个符号链接。
    fn sample() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let a = b"hello asar".to_vec();
        let b = b"console.log('b')".to_vec();
        let body = [a.clone(), b.clone()].concat();
        let header = serde_json::json!({
            "files": {
                "a.txt": {"size": a.len(), "offset": "0", "integrity": integrity_value(&a)},
                "dir": {"files": {
                    "b.js": {"size": b.len(), "offset": a.len().to_string(), "integrity": integrity_value(&b), "executable": true},
                    "c.node": {"size": 5, "unpacked": true},
                    "ln": {"link": "a.txt"}
                }}
            }
        });
        let json = serde_json::to_string(&header).unwrap();
        let mut bytes = encode_header(&json);
        bytes.extend_from_slice(&body);
        (bytes, a, b)
    }

    #[test]
    fn parses_and_reads_entries() {
        let (bytes, a, b) = sample();
        let asar = Asar::from_bytes(bytes).unwrap();
        assert_eq!(asar.read("/a.txt").unwrap(), a);
        assert_eq!(asar.read("dir/b.js").unwrap(), b);
        assert!(asar.read("/dir/c.node").is_err(), "unpacked 不可读");
        assert!(asar.read("/dir/ln").is_err(), "符号链接不可读");
        let paths: Vec<_> = asar.entries().into_iter().map(|e| e.path).collect();
        assert_eq!(paths, vec!["/a.txt", "/dir/b.js", "/dir/c.node", "/dir/ln"]);
        assert!(asar.entry("/dir/b.js").unwrap().is_packed());
        assert!(!asar.entry("/dir/c.node").unwrap().is_packed());
    }

    #[test]
    fn header_pickle_alignment_matches_electron() {
        // 78161 字节的 JSON 在真实 Claude 包里对应 payloadSize=78168、headerSize=78172。
        let json = "x".repeat(78161);
        let enc = encode_header(&json);
        assert_eq!(u32le(&enc, 0).unwrap(), 4);
        assert_eq!(u32le(&enc, 4).unwrap(), 78172);
        assert_eq!(u32le(&enc, 8).unwrap(), 78168);
        assert_eq!(u32le(&enc, 12).unwrap(), 78161);
        assert_eq!(enc.len(), 8 + 78172);
    }

    #[test]
    fn replace_remove_and_save_roundtrip() {
        let (bytes, a, _b) = sample();
        let mut asar = Asar::from_bytes(bytes).unwrap();
        let new_b = b"console.log('patched, longer than before')".to_vec();
        asar.replace("/dir/b.js", new_b.clone()).unwrap();
        assert!(asar.remove("/dir/ln").unwrap());
        assert!(!asar.remove("/nope").unwrap());
        assert!(asar.replace("/dir/c.node", vec![]).is_err(), "unpacked 不能替换");
        assert!(asar.is_dirty());

        let tmp = std::env::temp_dir().join(format!("claudecn-asar-test-{}.asar", std::process::id()));
        let report = asar.save_to(&tmp).unwrap();
        assert_eq!(report.packed_files, 2);

        let re = Asar::open(&tmp).unwrap();
        assert_eq!(re.read("/a.txt").unwrap(), a);
        assert_eq!(re.read("/dir/b.js").unwrap(), new_b);
        assert!(re.entry("/dir/ln").is_none(), "符号链接应已删除");
        let b_node = node_at(re.header(), "/dir/b.js").unwrap();
        assert_eq!(b_node["size"], Value::from(new_b.len() as u64));
        assert_eq!(b_node["integrity"]["hash"], Value::String(sha256_hex(&new_b)));
        assert_eq!(b_node["executable"], Value::Bool(true), "无关字段保留");
        // 偏移连续：a 在 0，b 紧随其后
        assert_eq!(re.entry("/a.txt").unwrap().offset, Some(0));
        assert_eq!(re.entry("/dir/b.js").unwrap().offset, Some(a.len() as u64));
        // 头部 hash 与实际写入一致
        let json_len = u32le(&std::fs::read(&tmp).unwrap(), 12).unwrap() as usize;
        let json = String::from_utf8(std::fs::read(&tmp).unwrap()[16..16 + json_len].to_vec()).unwrap();
        assert_eq!(report.header_sha256, sha256_hex(json.as_bytes()));
        assert_eq!(asar.planned_header_sha256().unwrap(), report.header_sha256);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn integrity_blocks_follow_4mb_chunks() {
        let big = vec![7u8; BLOCK_SIZE + 10];
        let v = integrity_value(&big);
        assert_eq!(v["blocks"].as_array().unwrap().len(), 2);
        assert_eq!(v["blockSize"], Value::from(BLOCK_SIZE as u64));
        assert_eq!(v["blocks"][0], Value::String(sha256_hex(&big[..BLOCK_SIZE])));
        let empty = integrity_value(b"");
        assert_eq!(empty["blocks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn rejects_garbage() {
        assert!(Asar::from_bytes(vec![0; 8]).is_err());
        assert!(Asar::from_bytes(vec![9; 64]).is_err());
    }
}
