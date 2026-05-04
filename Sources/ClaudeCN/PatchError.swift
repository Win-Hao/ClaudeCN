import Foundation

enum PatchError: LocalizedError {
    case claudeNotFound
    case noBackupFound
    case resourcesNotFound
    case languageFileNotFound(String)
    case languageFileCorrupted
    case translationBundleMissing
    case scriptFailed
    case privilegedCommandFailed(String)

    var errorDescription: String? {
        switch self {
        case .claudeNotFound:
            return "未找到 Claude Desktop，请确认已安装到 /Applications 目录。"
        case .noBackupFound:
            return "未找到备份文件，无法还原。"
        case .resourcesNotFound:
            return "Claude Desktop 资源目录不存在。"
        case .languageFileNotFound(let name):
            return "未找到语言文件：\(name)，Claude 版本可能不兼容。"
        case .languageFileCorrupted:
            return "语言文件格式异常，无法解析。"
        case .translationBundleMissing:
            return "中文翻译包丢失，请重新下载 ClaudeCN。"
        case .scriptFailed:
            return "脚本执行失败。"
        case .privilegedCommandFailed(let message):
            return "操作失败：\(message)"
        }
    }
}
