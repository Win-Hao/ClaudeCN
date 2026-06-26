// 站点对外链接的唯一来源。换仓库 / 换下载入口，只改这一处。
export const GITHUB_URL = 'https://github.com/Win-Hao/ClaudeCN'
export const RELEASES_URL = 'https://github.com/Win-Hao/ClaudeCN/releases/latest'
// macOS dmg 兜底直链：页面优先用 GitHub API 取「最新 release 的 .dmg」（见 hooks/useLatestDmg.ts），
// 仅在拉取失败 / 限流时回退到这里。发新版后顺手更新它即可。
export const DOWNLOAD_MAC = 'https://github.com/Win-Hao/ClaudeCN/releases/download/v2.0.1/ClaudeCN_2.0.1_aarch64.dmg'
// Windows：用户去 Releases 页手动下载（最新 release 不一定带 .exe）。
export const DOWNLOAD_WIN = RELEASES_URL
export const CHANGELOG_URL = 'https://github.com/Win-Hao/ClaudeCN/releases'
export const ISSUES_URL = 'https://github.com/Win-Hao/ClaudeCN/issues'
export const PROFILE_URL = 'https://github.com/Win-Hao'
export const LICENSE_URL = 'https://github.com/Win-Hao/ClaudeCN/blob/main/LICENSE'
