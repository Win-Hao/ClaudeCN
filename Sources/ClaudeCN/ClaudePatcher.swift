import Foundation
import AppKit

struct ClaudePatcher {
    static let claudeAppPath = "/Applications/Claude.app"
    private static let configDir = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/Claude")
    private static let configFile = configDir.appendingPathComponent("config.json")
    private static let backupDir = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/ClaudeCN/backups").path
    private static let backupSuffix = "backup-before-zh-CN"

    private static let frontendI18nRel = "Contents/Resources/ion-dist/i18n"
    private static let frontendAssetsRel = "Contents/Resources/ion-dist/assets/v1"
    private static let desktopResourcesRel = "Contents/Resources"

    func getClaudeVersion() -> String? {
        let plistPath = Self.claudeAppPath + "/Contents/Info.plist"
        guard let plist = NSDictionary(contentsOfFile: plistPath),
              let version = plist["CFBundleShortVersionString"] as? String else {
            return nil
        }
        return version
    }

    private func readConfigJSON() -> [String: Any]? {
        guard let data = FileManager.default.contents(atPath: Self.configFile.path),
              let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }
        return dict
    }

    func isPatched() -> Bool {
        guard let config = readConfigJSON(),
              let locale = config["locale"] as? String else {
            return false
        }
        return locale == "zh-CN"
    }

    // MARK: - Backup


    func restore() throws {
        let backupZip = Self.backupDir + "/Claude-original.zip"
        let fm = FileManager.default

        guard fm.fileExists(atPath: backupZip) else {
            throw PatchError.noBackupFound
        }

        let tempDir = NSTemporaryDirectory() + "ClaudeCN-restore-" + UUID().uuidString
        defer { try? fm.removeItem(atPath: tempDir) }
        try fm.createDirectory(atPath: tempDir, withIntermediateDirectories: true)

        let unzip = Process()
        unzip.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        unzip.arguments = ["-xk", backupZip, tempDir]
        try unzip.run()
        unzip.waitUntilExit()
        guard unzip.terminationStatus == 0 else {
            throw PatchError.privilegedCommandFailed("解压备份失败")
        }

        try runPrivilegedBatch(commands: [
            ["rm", "-rf", Self.claudeAppPath],
            ["mv", tempDir + "/Claude.app", Self.claudeAppPath],
        ])

        removeLocaleConfig()
    }

    // MARK: - Patch

    mutating func applyPatch() throws {
        let fm = FileManager.default
        let appPath = Self.claudeAppPath

        guard fm.fileExists(atPath: appPath) else {
            throw PatchError.claudeNotFound
        }

        try fm.createDirectory(atPath: Self.backupDir, withIntermediateDirectories: true)

        let backupZip = Self.backupDir + "/Claude-original.zip"
        if !fm.fileExists(atPath: backupZip) {
            let zip = Process()
            zip.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
            zip.arguments = ["-ck", "--keepParent", appPath, backupZip]
            try zip.run()
            zip.waitUntilExit()
            guard zip.terminationStatus == 0 else {
                throw PatchError.privilegedCommandFailed("备份压缩失败")
            }
        }

        let tempDir = NSTemporaryDirectory() + "ClaudeCN-patch-" + UUID().uuidString
        let tempApp = tempDir + "/Claude.app"

        defer { try? fm.removeItem(atPath: tempDir) }

        try fm.createDirectory(atPath: tempDir, withIntermediateDirectories: true)

        try runPrivilegedBatch(commands: [
            ["cp", "-R", appPath, tempApp],
            ["chown", "-R", NSUserName(), tempDir],
        ])

        try patchLanguageWhitelist(appPath: tempApp)
        try mergeFrontendLocale(appPath: tempApp)
        try installDesktopLocale(appPath: tempApp)
        try installStatsigLocale(appPath: tempApp)
        try resignApp(appPath: tempApp)
        try clearQuarantine(appPath: tempApp)

        try runPrivilegedBatch(commands: [
            ["rm", "-rf", Self.claudeAppPath],
            ["mv", tempApp, Self.claudeAppPath],
        ])

        try writeLocaleConfig()
    }

    // MARK: - Step 1: Patch language whitelist in index-*.js

    private func patchLanguageWhitelist(appPath: String) throws {
        let assetsDir = appPath + "/" + Self.frontendAssetsRel
        let fm = FileManager.default

        guard fm.fileExists(atPath: assetsDir) else {
            throw PatchError.resourcesNotFound
        }

        let files = try fm.contentsOfDirectory(atPath: assetsDir)
            .filter { $0.hasPrefix("index-") && $0.hasSuffix(".js") }
            .sorted()

        guard !files.isEmpty else {
            throw PatchError.languageFileNotFound("index-*.js")
        }

        let pattern = #"\["en-US","de-DE","fr-FR","ko-KR","ja-JP","es-419","es-ES","it-IT","hi-IN","pt-BR","id-ID"(.*?)\]"#
        let regex = try NSRegularExpression(pattern: pattern)
        let replacement = #"["en-US","de-DE","fr-FR","ko-KR","ja-JP","es-419","es-ES","it-IT","hi-IN","pt-BR","id-ID","zh-CN"]"#

        for file in files {
            let fullPath = assetsDir + "/" + file
            var text = try String(contentsOfFile: fullPath, encoding: .utf8)

            if text.contains("\"zh-CN\"") { continue }

            let range = NSRange(text.startIndex..., in: text)
            let patched = regex.stringByReplacingMatches(in: text, range: range, withTemplate: NSRegularExpression.escapedTemplate(for: replacement))

            if patched != text {
                try patched.write(toFile: fullPath, atomically: true, encoding: .utf8)
                return
            }
        }
    }

    // MARK: - Step 2: Merge frontend translation

    private func findZstd() -> String? {
        for path in ["/opt/homebrew/bin/zstd", "/usr/local/bin/zstd", "/usr/bin/zstd"] {
            if FileManager.default.fileExists(atPath: path) { return path }
        }
        let which = Process()
        which.executableURL = URL(fileURLWithPath: "/usr/bin/which")
        which.arguments = ["zstd"]
        let pipe = Pipe()
        which.standardOutput = pipe
        which.standardError = Pipe()
        try? which.run()
        which.waitUntilExit()
        let output = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return (output?.isEmpty == false) ? output : nil
    }

    private func mergeFrontendLocale(appPath: String) throws {
        let i18nDir = appPath + "/" + Self.frontendI18nRel
        let enUSPath = i18nDir + "/en-US.json"

        guard let enData = FileManager.default.contents(atPath: enUSPath),
              let enDict = try JSONSerialization.jsonObject(with: enData) as? [String: Any] else {
            throw PatchError.languageFileCorrupted
        }

        let zhTranslation = try loadBundledTranslation()
        let merged = mergeTranslations(base: enDict, overlay: zhTranslation)
        let mergedData = try JSONSerialization.data(withJSONObject: merged, options: [.prettyPrinted, .sortedKeys])

        let zhCNPath = i18nDir + "/zh-CN.json"
        try mergedData.write(to: URL(fileURLWithPath: zhCNPath))

        if let zstdPath = findZstd() {
            let zstPath = zhCNPath + ".zst"
            let zstProcess = Process()
            zstProcess.executableURL = URL(fileURLWithPath: zstdPath)
            zstProcess.arguments = ["-f", "--rm", zhCNPath, "-o", zstPath]
            try? zstProcess.run()
            zstProcess.waitUntilExit()

            try mergedData.write(to: URL(fileURLWithPath: zhCNPath))

            let overridesPath = i18nDir + "/zh-CN.overrides.json"
            let overridesZstPath = overridesPath + ".zst"
            try "{}".write(toFile: overridesPath, atomically: true, encoding: .utf8)

            let zst2 = Process()
            zst2.executableURL = URL(fileURLWithPath: zstdPath)
            zst2.arguments = ["-f", "--rm", overridesPath, "-o", overridesZstPath]
            try? zst2.run()
            zst2.waitUntilExit()

            try "{}".write(toFile: overridesPath, atomically: true, encoding: .utf8)
        }
    }

    // MARK: - Step 3: Install desktop locale

    private func installDesktopLocale(appPath: String) throws {
        let resourcesDir = appPath + "/" + Self.desktopResourcesRel
        let fm = FileManager.default

        if let desktopURL = Bundle.main.url(forResource: "desktop-zh-CN", withExtension: "json") {
            let target = resourcesDir + "/zh-CN.json"
            if fm.fileExists(atPath: target) { try fm.removeItem(atPath: target) }
            try fm.copyItem(at: desktopURL, to: URL(fileURLWithPath: target))
        }

        if let stringsURL = Bundle.main.url(forResource: "claude-Localizable", withExtension: "strings") {
            for folder in ["zh-CN.lproj", "zh_CN.lproj"] {
                let outDir = resourcesDir + "/" + folder
                try fm.createDirectory(atPath: outDir, withIntermediateDirectories: true)
                let target = outDir + "/Localizable.strings"
                if fm.fileExists(atPath: target) { try fm.removeItem(atPath: target) }
                try fm.copyItem(at: stringsURL, to: URL(fileURLWithPath: target))
            }
        }
    }

    // MARK: - Step 4: Install statsig locale

    private func installStatsigLocale(appPath: String) throws {
        let statsigDir = appPath + "/" + Self.frontendI18nRel + "/statsig"
        let fm = FileManager.default

        guard fm.fileExists(atPath: statsigDir) else { return }

        let target = statsigDir + "/zh-CN.json"
        if fm.fileExists(atPath: target) { try fm.removeItem(atPath: target) }

        if let bundled = Bundle.main.url(forResource: "statsig-zh-CN", withExtension: "json") {
            try fm.copyItem(at: bundled, to: URL(fileURLWithPath: target))
        } else {
            let enUS = statsigDir + "/en-US.json"
            if fm.fileExists(atPath: enUS) {
                try fm.copyItem(atPath: enUS, toPath: target)
            }
        }
    }

    // MARK: - Step 5: Resign

    private mutating func resignApp(appPath: String) throws {
        let contents = appPath + "/Contents"
        let fm = FileManager.default
        var fileTargets: [String] = []
        var bundleTargets: [String] = []

        if let enumerator = fm.enumerator(atPath: contents) {
            while let item = enumerator.nextObject() as? String {
                let fullPath = contents + "/" + item
                var isDir: ObjCBool = false
                fm.fileExists(atPath: fullPath, isDirectory: &isDir)

                if isDir.boolValue {
                    if item.hasSuffix(".app") || item.hasSuffix(".framework") {
                        bundleTargets.append(fullPath)
                    }
                } else {
                    if item.hasSuffix(".dylib") || item.hasSuffix(".node") || item.hasSuffix(".so") ||
                       fm.isExecutableFile(atPath: fullPath) {
                        fileTargets.append(fullPath)
                    }
                }
            }
        }

        fileTargets.sort { $0.components(separatedBy: "/").count > $1.components(separatedBy: "/").count }
        for path in fileTargets {
            try codesign(path)
        }

        bundleTargets.sort { $0.components(separatedBy: "/").count > $1.components(separatedBy: "/").count }
        for path in bundleTargets {
            try codesign(path)
        }

        try codesign(appPath)
    }

    private var entitlementsTempDir: String = ""

    private mutating func codesign(_ path: String) throws {
        if entitlementsTempDir.isEmpty {
            entitlementsTempDir = NSTemporaryDirectory() + "claudecn-entitlements-" + UUID().uuidString
            try FileManager.default.createDirectory(atPath: entitlementsTempDir,
                                                     withIntermediateDirectories: true)
        }

        let entitlements = loadEntitlements(path)

        var cmd = ["/usr/bin/codesign", "--force", "--sign", "-",
                   "--options", "runtime", "--preserve-metadata=identifier,flags"]

        if var ent = entitlements {
            ent["com.apple.security.cs.disable-library-validation"] = true
            let plistPath = entitlementsTempDir + "/\(abs(path.hashValue)).plist"
            let plistData = try PropertyListSerialization.data(
                fromPropertyList: ent, format: .xml, options: 0)
            try plistData.write(to: URL(fileURLWithPath: plistPath))
            cmd += ["--entitlements", plistPath]
        }

        cmd.append(path)

        let process = Process()
        process.executableURL = URL(fileURLWithPath: cmd[0])
        process.arguments = Array(cmd.dropFirst())
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw PatchError.privilegedCommandFailed("重签名失败: \(path)")
        }
    }

    private func loadEntitlements(_ path: String) -> [String: Any]? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/codesign")
        process.arguments = ["-d", "--entitlements", ":-", path]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = Pipe()
        try? process.run()
        process.waitUntilExit()

        guard process.terminationStatus == 0 else { return nil }

        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        guard !data.isEmpty else { return nil }

        guard let plist = try? PropertyListSerialization.propertyList(
            from: data, format: nil) as? [String: Any] else {
            return nil
        }
        return plist
    }

    private func clearQuarantine(appPath: String) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/xattr")
        process.arguments = ["-dr", "com.apple.quarantine", appPath]
        try process.run()
        process.waitUntilExit()
    }

    func quitClaude() {
        let quit = Process()
        quit.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        quit.arguments = ["-e", "tell application \"Claude\" to quit"]
        try? quit.run()
        quit.waitUntilExit()
    }

    func launchClaude() {
        NSWorkspace.shared.open(URL(fileURLWithPath: Self.claudeAppPath))
    }

    // MARK: - Helpers

    private func loadBundledTranslation() throws -> [String: Any] {
        guard let url = Bundle.main.url(forResource: "zh-CN", withExtension: "json") else {
            throw PatchError.translationBundleMissing
        }
        let data = try Data(contentsOf: url)
        guard let dict = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw PatchError.languageFileCorrupted
        }
        return dict
    }

    private func mergeTranslations(base: [String: Any], overlay: [String: Any]) -> [String: Any] {
        var result = base
        for (key, value) in overlay {
            if let baseDict = base[key] as? [String: Any],
               let overlayDict = value as? [String: Any] {
                result[key] = mergeTranslations(base: baseDict, overlay: overlayDict)
            } else {
                result[key] = value
            }
        }
        return result
    }

    func writeLocaleConfig() throws {
        let fm = FileManager.default
        if !fm.fileExists(atPath: Self.configDir.path) {
            try fm.createDirectory(at: Self.configDir, withIntermediateDirectories: true)
        }

        var config: [String: Any] = [:]
        if let data = fm.contents(atPath: Self.configFile.path),
           let existing = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            config = existing
        }

        config["locale"] = "zh-CN"
        let data = try JSONSerialization.data(withJSONObject: config, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: Self.configFile)

        UserDefaults.standard.removePersistentDomain(forName: "com.anthropic.Claude")
        let defaults = UserDefaults(suiteName: "com.anthropic.Claude")
        defaults?.set(["zh-CN", "zh-Hans", "en"], forKey: "AppleLanguages")
        defaults?.synchronize()
    }

    private func removeLocaleConfig() {
        guard let data = FileManager.default.contents(atPath: Self.configFile.path),
              var config = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return
        }
        config.removeValue(forKey: "locale")
        if let newData = try? JSONSerialization.data(withJSONObject: config, options: [.prettyPrinted, .sortedKeys]) {
            try? newData.write(to: Self.configFile)
        }

        let defaults = UserDefaults(suiteName: "com.anthropic.Claude")
        defaults?.removeObject(forKey: "AppleLanguages")
        defaults?.synchronize()
    }

    func hasBackup() -> Bool {
        FileManager.default.fileExists(atPath: Self.backupDir + "/Claude-original.zip")
    }

    private func runPrivilegedBatch(commands: [[String]]) throws {
        let joined = commands.map { cmd in
            cmd.map { $0.contains(" ") ? "'\($0)'" : $0 }.joined(separator: " ")
        }.joined(separator: " && ")
        try runPrivilegedRaw(joined)
    }

    private func runPrivileged(command: String, arguments: [String]) throws {
        let fullCommand = ([command] + arguments)
            .map { $0.contains(" ") ? "'\($0)'" : $0 }
            .joined(separator: " ")
        try runPrivilegedRaw(fullCommand)
    }

    private func runPrivilegedRaw(_ shellCommand: String) throws {
        let script = "do shell script \"\(shellCommand)\" with administrator privileges"
        guard let appleScript = NSAppleScript(source: script) else {
            throw PatchError.scriptFailed
        }

        var error: NSDictionary?
        appleScript.executeAndReturnError(&error)
        if let error = error {
            let message = error[NSAppleScript.errorMessage] as? String ?? "未知错误"
            throw PatchError.privilegedCommandFailed(message)
        }
    }
}
