import SwiftUI

/// Diagnostic information panel shown in LOCAL_MODE Troubleshooting settings.
/// Displays local environment status, storage info, and local-mode specific health checks.
struct LocalDiagnosticsView: View {
  @ObservedObject var appState: AppState

  @State private var isCheckingStorage = false
  @State private var storageInfo: StorageInfo? = nil
  @State private var isRunningDiagnostics = false
  @State private var diagnosticResults: [DiagnosticCheck] = []

  var body: some View {
    VStack(spacing: 20) {
      localModeStatusBanner

      diagnosticsCard

      if !diagnosticResults.isEmpty {
        diagnosticResultsList
      }

      storageCard

      localModeInfoCard
    }
    .padding(.vertical, 8)
    .onAppear {
      loadStorageInfo()
      runLocalDiagnostics()
    }
  }

  // MARK: - Local Mode Status Banner

  private var localModeStatusBanner: some View {
    HStack(spacing: 12) {
      Circle()
        .fill(OmiColors.success.opacity(0.2))
        .frame(width: 12, height: 12)
        .overlay(
          Circle()
            .fill(OmiColors.success)
            .frame(width: 6, height: 6)
        )

      VStack(alignment: .leading, spacing: 2) {
        Text("Running in Local Mode")
          .font(.system(size: 14, weight: .semibold))
          .foregroundColor(OmiColors.textPrimary)

        Text("All data stays on this Mac — no cloud account required")
          .font(.system(size: 12))
          .foregroundColor(OmiColors.textTertiary)
      }

      Spacer()
    }
    .padding(16)
    .background(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .fill(OmiColors.success.opacity(0.08))
        .overlay(
          RoundedRectangle(cornerRadius: 12, style: .continuous)
            .stroke(OmiColors.success.opacity(0.2), lineWidth: 1)
        )
    )
  }

  // MARK: - Diagnostics Card

  private var diagnosticsCard: some View {
    settingsCard(settingId: "local.diagnostics.run") {
      VStack(alignment: .leading, spacing: 14) {
        HStack(spacing: 12) {
          Image(systemName: "stethoscope")
            .font(.system(size: 16))
            .foregroundColor(OmiColors.purplePrimary)
            .frame(width: 24, height: 24)

          VStack(alignment: .leading, spacing: 4) {
            Text("Local Health Check")
              .font(.system(size: 15, weight: .semibold))
              .foregroundColor(OmiColors.textPrimary)

            Text("Verify local mode is functioning correctly")
              .font(.system(size: 12))
              .foregroundColor(OmiColors.textTertiary)
          }

          Spacer()

          Button(action: runLocalDiagnostics) {
            if isRunningDiagnostics {
              ProgressView()
                .scaleEffect(0.7)
            } else {
              Text("Run Check")
                .font(.system(size: 13, weight: .medium))
                .foregroundColor(.white)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(
                  RoundedRectangle(cornerRadius: 6)
                    .fill(OmiColors.purplePrimary)
                )
            }
          }
          .buttonStyle(.plain)
          .disabled(isRunningDiagnostics)
        }
      }
    }
  }

  // MARK: - Diagnostic Results

  private var diagnosticResultsList: some View {
    VStack(spacing: 10) {
      ForEach(diagnosticResults) { result in
        diagnosticResultRow(result)
      }
    }
  }

  private func diagnosticResultRow(_ result: DiagnosticCheck) -> some View {
    settingsCard(settingId: "local.diagnostics.\(result.id)") {
      HStack(spacing: 12) {
        Image(systemName: result.icon)
          .font(.system(size: 14))
          .foregroundColor(result.color)
          .frame(width: 20)

        VStack(alignment: .leading, spacing: 2) {
          Text(result.name)
            .font(.system(size: 14, weight: .medium))
            .foregroundColor(OmiColors.textPrimary)

          Text(result.detail)
            .font(.system(size: 12))
            .foregroundColor(OmiColors.textTertiary)
        }

        Spacer()

        Text(result.status)
          .font(.system(size: 12, weight: .medium))
          .foregroundColor(result.color)
      }
    }
  }

  // MARK: - Storage Card

  private var storageCard: some View {
    settingsCard(settingId: "local.diagnostics.storage") {
      VStack(alignment: .leading, spacing: 14) {
        HStack(spacing: 12) {
          Image(systemName: "internaldrive.fill")
            .font(.system(size: 16))
            .foregroundColor(OmiColors.purplePrimary)
            .frame(width: 24, height: 24)

          VStack(alignment: .leading, spacing: 4) {
            Text("Local Storage")
              .font(.system(size: 15, weight: .semibold))
              .foregroundColor(OmiColors.textPrimary)

            if isCheckingStorage {
              ProgressView()
                .scaleEffect(0.6)
            } else if let info = storageInfo {
              Text("\(info.usedFormatted) used of \(info.totalFormatted)")
                .font(.system(size: 12))
                .foregroundColor(OmiColors.textTertiary)
            }
          }

          Spacer()

          if let info = storageInfo {
            Text(info.percentUsed)
              .font(.system(size: 12, weight: .medium))
              .foregroundColor(info.percentUsedValue < 90 ? OmiColors.success : OmiColors.warning)
          }
        }

        if let info = storageInfo {
          ProgressView(value: min(Double(info.percentUsedValue) / 100.0, 1.0))
            .progressViewStyle(LinearProgressViewStyle(tint: info.percentUsedValue < 90 ? OmiColors.purplePrimary : OmiColors.warning))
            .frame(height: 4)
        }
      }
    }
  }

  // MARK: - Local Mode Info Card

  private var localModeInfoCard: some View {
    settingsCard(settingId: "local.diagnostics.info") {
      VStack(alignment: .leading, spacing: 14) {
        HStack(spacing: 12) {
          Image(systemName: "info.circle.fill")
            .font(.system(size: 16))
            .foregroundColor(OmiColors.textTertiary)
            .frame(width: 24, height: 24)

          Text("About Local Mode")
            .font(.system(size: 15, weight: .semibold))
            .foregroundColor(OmiColors.textPrimary)
        }

        VStack(alignment: .leading, spacing: 10) {
          infoRow(label: "Auth", value: "Local stored tokens (no Firebase)")
          infoRow(label: "Storage", value: "This Mac only")
          infoRow(label: "Cloud Sync", value: "Disabled")
          infoRow(label: "API", value: localApiUrl)
        }
      }
    }
  }

  private func infoRow(label: String, value: String) -> some View {
    HStack(spacing: 12) {
      Text(label)
        .font(.system(size: 13))
        .foregroundColor(OmiColors.textTertiary)
        .frame(width: 80, alignment: .leading)

      Text(value)
        .font(.system(size: 13, weight: .medium))
        .foregroundColor(OmiColors.textPrimary)

      Spacer()
    }
  }

  private var localApiUrl: String {
    if let url = getenv("OMI_API_URL"), let str = String(validatingUTF8: url), !str.isEmpty {
      return str
    }
    return "Not configured"
  }

  // MARK: - Helpers

  private var settingsCard: some View {
    { @ViewBuilder content: () -> some View in
      content()
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(20)
        .background(
          RoundedRectangle(cornerRadius: 12, style: .continuous)
            .fill(OmiColors.backgroundTertiary.opacity(0.5))
            .overlay(
              RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(OmiColors.backgroundQuaternary.opacity(0.3), lineWidth: 1)
            )
        )
    }
  }

  // MARK: - Data Loading

  private func loadStorageInfo() {
    isCheckingStorage = true
    Task {
      let fm = FileManager.default
      do {
        let attrs = try fm.attributesOfFileSystem(forPath: NSHomeDirectory())
        let totalSpace = attrs[.systemSize] as? Int64 ?? 0
        let freeSpace = attrs[.systemFreeSize] as? Int64 ?? 0
        let usedSpace = totalSpace - freeSpace
        let percentUsed = totalSpace > 0 ? Int((Double(usedSpace) / Double(totalSpace)) * 100) : 0

        await MainActor.run {
          storageInfo = StorageInfo(
            total: totalSpace,
            used: usedSpace,
            free: freeSpace,
            percentUsedValue: percentUsed
          )
          isCheckingStorage = false
        }
      } catch {
        await MainActor.run {
          isCheckingStorage = false
        }
      }
    }
  }

  private func runLocalDiagnostics() {
    isRunningDiagnostics = true
    diagnosticResults = []

    var results: [DiagnosticCheck] = []

    // Check 1: LOCAL_MODE flag
    let localModeEnabled = ProcessInfo.processInfo.environment["LOCAL_MODE"] == "1"
    results.append(DiagnosticCheck(
      id: "local_mode_flag",
      name: "Local Mode Flag",
      status: localModeEnabled ? "Active" : "Inactive",
      detail: localModeEnabled ? "LOCAL_MODE=1 is set" : "LOCAL_MODE is not set",
      icon: localModeEnabled ? "checkmark.circle.fill" : "xmark.circle.fill",
      color: localModeEnabled ? OmiColors.success : OmiColors.warning
    ))

    // Check 2: Auth token storage
    let hasStoredAuth = UserDefaults.standard.bool(forKey: "auth_isSignedIn")
    results.append(DiagnosticCheck(
      id: "auth_token",
      name: "Auth Token",
      status: hasStoredAuth ? "Stored" : "Missing",
      detail: hasStoredAuth ? "Local auth credentials found in UserDefaults" : "No stored auth — may need sign-in",
      icon: hasStoredAuth ? "key.fill" : "key.slash.fill",
      color: hasStoredAuth ? OmiColors.success : OmiColors.warning
    ))

    // Check 3: API URL configured
    let apiUrlConfigured = !localApiUrl.isEmpty && localApiUrl != "Not configured"
    results.append(DiagnosticCheck(
      id: "api_url",
      name: "API URL",
      status: apiUrlConfigured ? "Configured" : "Missing",
      detail: apiUrlConfigured ? "OMI_API_URL points to \(localApiUrl)" : "OMI_API_URL not set in environment",
      icon: apiUrlConfigured ? "link.circle.fill" : "link.badge.plus",
      color: apiUrlConfigured ? OmiColors.purplePrimary : OmiColors.warning
    ))

    // Check 4: Storage availability
    let storageOk = storageInfo != nil && storageInfo!.percentUsedValue < 95
    results.append(DiagnosticCheck(
      id: "storage",
      name: "Storage",
      status: storageOk ? "Available" : "Low",
      detail: storageOk ? "\(storageInfo!.percentUsed) disk used" : "Disk space critically low",
      icon: storageOk ? "internaldrive.fill" : "exclamationmark.triangle.fill",
      color: storageOk ? OmiColors.success : OmiColors.warning
    ))

    diagnosticResults = results
    isRunningDiagnostics = false
  }
}

// MARK: - Data Models

struct StorageInfo {
  let total: Int64
  let used: Int64
  let free: Int64
  let percentUsedValue: Int

  var totalFormatted: String { ByteCountFormatter.string(fromBytes: total, countStyle: .file) }
  var usedFormatted: String { ByteCountFormatter.string(fromBytes: used, countStyle: .file) }
  var freeFormatted: String { ByteCountFormatter.string(fromBytes: free, countStyle: .file) }
  var percentUsed: String { "\(percentUsedValue)% used" }
}

struct DiagnosticCheck: Identifiable {
  let id: String
  let name: String
  let status: String
  let detail: String
  let icon: String
  let color: Color
}
