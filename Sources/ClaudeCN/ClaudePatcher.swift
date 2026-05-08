import Foundation
import AppKit

struct ClaudePatcher {
    static let claudeAppPath = "/Applications/Claude.app"
    private static let configDir = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/Claude")
    private static let configFile = configDir.appendingPathComponent("config.json")
    private static let backupDir = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/ClaudeCN/backups").path
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
              let locale = config["locale"] as? String,
              locale == "zh-CN" else {
            return false
        }

        let fm = FileManager.default
        let i18nZh = Self.claudeAppPath + "/" + Self.frontendI18nRel + "/zh-CN.json"
        guard fm.fileExists(atPath: i18nZh) else { return false }

        let assetsDir = Self.claudeAppPath + "/" + Self.frontendAssetsRel
        guard let files = try? fm.contentsOfDirectory(atPath: assetsDir) else { return false }
        let indexFiles = files.filter { $0.hasPrefix("index-") && $0.hasSuffix(".js") }
        guard !indexFiles.isEmpty else { return false }
        for file in indexFiles {
            guard let content = try? String(contentsOfFile: assetsDir + "/" + file, encoding: .utf8),
                  content.contains("\"zh-CN\"") else {
                return false
            }
        }
        return true
    }

    // MARK: - Backup


    func restore() throws {
        let backupZip = Self.backupDir + "/Claude-original.zip"
        let fm = FileManager.default

        guard fm.fileExists(atPath: backupZip) else {
            throw PatchError.noBackupFound
        }

        let tempDir = NSTemporaryDirectory() + "ClaudeCN-restore-" + UUID().uuidString
        try fm.createDirectory(atPath: tempDir, withIntermediateDirectories: true)

        let unzip = Process()
        unzip.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        unzip.arguments = ["-xk", backupZip, tempDir]
        try unzip.run()
        unzip.waitUntilExit()
        guard unzip.terminationStatus == 0 else {
            try? fm.removeItem(atPath: tempDir)
            throw PatchError.privilegedCommandFailed("解压备份失败")
        }

        let trashPath = NSTemporaryDirectory() + "ClaudeCN-old-" + UUID().uuidString
        do {
            try runPrivilegedBatch(commands: [
                ["mv", Self.claudeAppPath, trashPath],
                ["mv", tempDir + "/Claude.app", Self.claudeAppPath],
            ])
        } catch {
            // 第二步 mv 失败时，把旧 app 移回来
            try? runPrivilegedRaw("mv '\(trashPath)' '\(Self.claudeAppPath)'")
            try? fm.removeItem(atPath: tempDir)
            throw error
        }
        try? fm.removeItem(atPath: tempDir)
        try? runPrivilegedRaw("rm -rf '\(trashPath)'")

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
        let backupVersionFile = Self.backupDir + "/backup-version.txt"
        let currentVersion = getClaudeVersion() ?? ""

        var needsBackup = !fm.fileExists(atPath: backupZip)
        if !needsBackup {
            let storedVersion = (try? String(contentsOfFile: backupVersionFile, encoding: .utf8))?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if storedVersion != currentVersion {
                try? fm.removeItem(atPath: backupZip)
                needsBackup = true
            }
        }

        if needsBackup {
            let zip = Process()
            zip.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
            zip.arguments = ["-ck", "--keepParent", appPath, backupZip]
            try zip.run()
            zip.waitUntilExit()
            guard zip.terminationStatus == 0 else {
                throw PatchError.privilegedCommandFailed("备份压缩失败")
            }
            try? currentVersion.write(toFile: backupVersionFile, atomically: true, encoding: .utf8)
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

        let trashPath = NSTemporaryDirectory() + "ClaudeCN-old-" + UUID().uuidString
        do {
            try runPrivilegedBatch(commands: [
                ["mv", Self.claudeAppPath, trashPath],
                ["mv", tempApp, Self.claudeAppPath],
            ])
        } catch {
            try? runPrivilegedRaw("mv '\(trashPath)' '\(Self.claudeAppPath)'")
            throw error
        }
        try? runPrivilegedRaw("rm -rf '\(trashPath)'")

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

        let patterns = [
            #"(\["en-US","de-DE","fr-FR","ko-KR","ja-JP","es-419","es-ES","it-IT","hi-IN","pt-BR","id-ID"[^\]]*)\]"#,
            #"(\["en-US"(?:,"[a-zA-Z]{2,3}(?:-[a-zA-Z0-9]{2,4})*")+)\]"#,
            #"(\["en-US"(?:\s*,\s*"[a-zA-Z]{2,3}(?:-[a-zA-Z0-9]{2,4})*")+\s*)\]"#,
        ]
        let regexes = patterns.compactMap { try? NSRegularExpression(pattern: $0) }

        var injected = false

        for file in files {
            let fullPath = assetsDir + "/" + file
            let text = try String(contentsOfFile: fullPath, encoding: .utf8)

            if text.contains("\"zh-CN\"") {
                injected = true
                continue
            }

            let range = NSRange(text.startIndex..., in: text)

            for regex in regexes {
                let result = regex.stringByReplacingMatches(
                    in: text, range: range, withTemplate: #"$1,"zh-CN"]"#)
                if result != text {
                    try result.write(toFile: fullPath, atomically: true, encoding: .utf8)
                    injected = true
                    break
                }
            }

            if injected { break }
        }

        if !injected {
            throw PatchError.languageFileNotFound("语言白名单")
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

    // MARK: - Step 5: Update ElectronTeamID + Resign

    private static let certName = "ClaudeCN Signing"

    private func ensureSigningCertificate() -> Bool {
        let check = Process()
        check.executableURL = URL(fileURLWithPath: "/usr/bin/security")
        check.arguments = ["find-identity", "-v", "-p", "codesigning"]
        let pipe = Pipe()
        check.standardOutput = pipe
        check.standardError = Pipe()
        try? check.run()
        check.waitUntilExit()
        let output = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        if output.contains("\"\(Self.certName)\"") { return true }

        let tmpDir = NSTemporaryDirectory() + "claudecn-cert-" + UUID().uuidString
        try? FileManager.default.createDirectory(atPath: tmpDir, withIntermediateDirectories: true)
        let keyPath = tmpDir + "/key.pem"
        let certPath = tmpDir + "/cert.pem"
        let p12Path = tmpDir + "/cert.p12"
        let cfgPath = tmpDir + "/cert.cfg"

        let cfg = """
        [req]
        default_bits = 2048
        prompt = no
        distinguished_name = dn
        x509_extensions = v3

        [dn]
        CN = \(Self.certName)

        [v3]
        keyUsage = critical, digitalSignature
        extendedKeyUsage = codeSigning
        basicConstraints = CA:false
        """
        try? cfg.write(toFile: cfgPath, atomically: true, encoding: .utf8)

        let gen = Process()
        gen.executableURL = URL(fileURLWithPath: "/usr/bin/openssl")
        gen.arguments = ["req", "-x509", "-newkey", "rsa:2048", "-keyout", keyPath,
                         "-out", certPath, "-days", "3650", "-nodes", "-config", cfgPath]
        gen.standardOutput = Pipe()
        gen.standardError = Pipe()
        try? gen.run()
        gen.waitUntilExit()
        guard gen.terminationStatus == 0 else {
            try? FileManager.default.removeItem(atPath: tmpDir)
            return false
        }

        let export = Process()
        export.executableURL = URL(fileURLWithPath: "/usr/bin/openssl")
        export.arguments = ["pkcs12", "-export", "-out", p12Path, "-inkey", keyPath,
                            "-in", certPath, "-passout", "pass:claudecn",
                            "-certpbe", "PBE-SHA1-3DES", "-keypbe", "PBE-SHA1-3DES", "-macalg", "sha1"]
        export.standardOutput = Pipe()
        export.standardError = Pipe()
        try? export.run()
        export.waitUntilExit()

        let imp = Process()
        imp.executableURL = URL(fileURLWithPath: "/usr/bin/security")
        imp.arguments = ["import", p12Path, "-k",
                         FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Keychains/login.keychain-db").path,
                         "-P", "claudecn", "-T", "/usr/bin/codesign", "-A"]
        imp.standardOutput = Pipe()
        imp.standardError = Pipe()
        try? imp.run()
        imp.waitUntilExit()

        let trust = Process()
        trust.executableURL = URL(fileURLWithPath: "/usr/bin/security")
        trust.arguments = ["add-trusted-cert", "-p", "codeSign", "-k",
                           FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Keychains/login.keychain-db").path,
                           certPath]
        trust.standardOutput = Pipe()
        trust.standardError = Pipe()
        try? trust.run()
        trust.waitUntilExit()

        try? FileManager.default.removeItem(atPath: tmpDir)
        return imp.terminationStatus == 0
    }

    private func updateElectronTeamID(appPath: String, teamID: String) throws {
        let plistPath = appPath + "/Contents/Info.plist"
        guard let plistData = FileManager.default.contents(atPath: plistPath),
              var plist = try PropertyListSerialization.propertyList(from: plistData, format: nil) as? [String: Any] else {
            return
        }
        plist["ElectronTeamID"] = teamID
        let newData = try PropertyListSerialization.data(fromPropertyList: plist, format: .xml, options: 0)
        try newData.write(to: URL(fileURLWithPath: plistPath))
    }

    private mutating func resignApp(appPath: String) throws {
        if ensureSigningCertificate() {
            var ent = extractEntitlements(appPath + "/Contents/MacOS/Claude") ?? [:]
            ent.removeValue(forKey: "com.apple.application-identifier")
            ent.removeValue(forKey: "com.apple.developer.team-identifier")
            ent["com.apple.security.cs.disable-library-validation"] = true

            try updateElectronTeamID(appPath: appPath, teamID: "not set")

            let tmpDir = NSTemporaryDirectory() + "claudecn-ent-" + UUID().uuidString
            try FileManager.default.createDirectory(atPath: tmpDir, withIntermediateDirectories: true)
            let plistPath = tmpDir + "/app.plist"
            let plistData = try PropertyListSerialization.data(fromPropertyList: ent, format: .xml, options: 0)
            try plistData.write(to: URL(fileURLWithPath: plistPath))

            let sign = Process()
            sign.executableURL = URL(fileURLWithPath: "/usr/bin/codesign")
            sign.arguments = ["--force", "--sign", Self.certName, "--options", "runtime",
                              "--entitlements", plistPath, "--deep", appPath]
            try sign.run()
            sign.waitUntilExit()
            try? FileManager.default.removeItem(atPath: tmpDir)
            guard sign.terminationStatus == 0 else {
                throw PatchError.privilegedCommandFailed("重签名失败")
            }
        } else {
            let sign = Process()
            sign.executableURL = URL(fileURLWithPath: "/usr/bin/codesign")
            sign.arguments = ["--force", "--sign", "-", "--deep", appPath]
            try sign.run()
            sign.waitUntilExit()
            guard sign.terminationStatus == 0 else {
                throw PatchError.privilegedCommandFailed("重签名失败")
            }
        }
    }

    private func extractEntitlements(_ path: String) -> [String: Any]? {
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
        return try? PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any]
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

    func isClaudeRunning() -> Bool {
        !NSRunningApplication.runningApplications(withBundleIdentifier: "com.anthropic.Claude").isEmpty
    }

    func launchClaude() {
        let lsregister = Process()
        lsregister.executableURL = URL(fileURLWithPath: "/usr/bin/find")
        lsregister.arguments = [
            "/System/Library/Frameworks/CoreServices.framework",
            "-name", "lsregister", "-type", "f"
        ]
        let pipe = Pipe()
        lsregister.standardOutput = pipe
        lsregister.standardError = Pipe()
        try? lsregister.run()
        lsregister.waitUntilExit()
        if let path = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines), !path.isEmpty {
            let reg = Process()
            reg.executableURL = URL(fileURLWithPath: path)
            reg.arguments = ["-f", Self.claudeAppPath]
            try? reg.run()
            reg.waitUntilExit()
        }

        let launch = Process()
        launch.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        launch.arguments = [Self.claudeAppPath]
        try? launch.run()
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

    private func runPrivilegedRaw(_ shellCommand: String) throws {
        let escaped = shellCommand
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        let script = "do shell script \"\(escaped)\" with administrator privileges"
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
