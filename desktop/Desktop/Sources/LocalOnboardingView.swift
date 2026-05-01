import SwiftUI

/// Onboarding step shown only in LOCAL_MODE — explains local-only operation,
/// skips cloud account setup, and displays local storage status.
struct LocalOnboardingIntroStepView: View {
  @ObservedObject var coordinator: OnboardingPagedIntroCoordinator
  @ObservedObject var graphViewModel: MemoryGraphViewModel
  let stepIndex: Int
  let totalSteps: Int
  let onContinue: () -> Void
  let onForceComplete: (() -> Void)?

  var body: some View {
    OnboardingStepScaffold(
      graphViewModel: graphViewModel,
      stepIndex: stepIndex,
      totalSteps: totalSteps,
      eyebrow: "Local Mode",
      title: "Your data stays on this Mac",
      description:
        "Omi is running in local mode — no cloud account required, no data leaves your device.",
      layoutMode: .split,
      rightPaneMode: .message(
        title: "100% Private",
        detail:
          "Everything stays on your Mac. No servers, no accounts, no tracking."
      ),
      rightPaneFooterText: coordinator.connectedContextSummary,
      onForceComplete: onForceComplete
    ) {
      VStack(alignment: .leading, spacing: 24) {
        localModeBadge

        TextField("Your name", text: $coordinator.draftName)
          .textFieldStyle(.plain)
          .padding(.horizontal, 16)
          .padding(.vertical, 14)
          .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
              .fill(OmiColors.backgroundSecondary)
              .overlay(
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                  .stroke(Color.white.opacity(0.08), lineWidth: 1)
              )
          )
          .foregroundColor(OmiColors.textPrimary)
          .frame(maxWidth: 320)

        if let error = coordinator.lastActionError {
          Text(error)
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(OmiColors.warning)
            .multilineTextAlignment(.center)
        }

        localFeatureList

        Button("Continue") {
          Task {
            await coordinator.confirmPreferredName()
            if coordinator.lastActionError == nil {
              onContinue()
            }
          }
        }
        .buttonStyle(OnboardingCardButtonStyle(isPrimary: true))
      }
      .frame(maxWidth: .infinity, alignment: .center)
      .onAppear {
        coordinator.clearLastActionError()
        coordinator.draftName = coordinator.preferredName
      }
    }
  }

  private var localModeBadge: some View {
    HStack(spacing: 8) {
      Circle()
        .fill(OmiColors.success.opacity(0.2))
        .frame(width: 10, height: 10)
        .overlay(
          Circle()
            .fill(OmiColors.success)
            .frame(width: 6, height: 6)
        )

      Text("Local Mode Active")
        .font(.system(size: 13, weight: .semibold))
        .foregroundColor(OmiColors.success)

      Spacer()
    }
    .padding(.horizontal, 16)
    .padding(.vertical, 10)
    .background(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .fill(OmiColors.success.opacity(0.08))
        .overlay(
          RoundedRectangle(cornerRadius: 10, style: .continuous)
            .stroke(OmiColors.success.opacity(0.2), lineWidth: 1)
        )
    )
  }

  private var localFeatureList: some View {
    VStack(alignment: .leading, spacing: 12) {
      localFeatureRow(
        icon: "lock.shield.fill",
        title: "No account required",
        detail: "Skip sign-in entirely"
      )
      localFeatureRow(
        icon: "cloud.slash.fill",
        title: "Data never leaves your Mac",
        detail: "Everything stored locally"
      )
      localFeatureRow(
        icon: "bolt.slash.fill",
        title: "Works offline",
        detail: "No internet required"
      )
    }
    .padding(.vertical, 4)
  }

  private func localFeatureRow(icon: String, title: String, detail: String) -> some View {
    HStack(spacing: 12) {
      Image(systemName: icon)
        .font(.system(size: 14, weight: .semibold))
        .foregroundColor(OmiColors.purplePrimary)
        .frame(width: 22)

      VStack(alignment: .leading, spacing: 2) {
        Text(title)
          .font(.system(size: 14, weight: .medium))
          .foregroundColor(OmiColors.textPrimary)

        Text(detail)
          .font(.system(size: 12))
          .foregroundColor(OmiColors.textTertiary)
      }

      Spacer()
    }
  }
}

/// Standalone local onboarding view — used when LOCAL_MODE=1 and the user has not
/// yet completed the local-focused onboarding flow.
struct LocalOnboardingView: View {
  @ObservedObject var appState: AppState
  @ObservedObject var chatProvider: ChatProvider
  var onComplete: (() -> Void)? = nil

  @StateObject private var graphViewModel = MemoryGraphViewModel()
  @State private var currentStep = 0
  @State private var localStepCount = 3

  private let steps = [
    "Name",
    "Permissions",
    "Done",
  ]

  var body: some View {
    ZStack {
      OmiColors.backgroundPrimary
        .ignoresSafeArea()

      Group {
        if appState.hasCompletedOnboarding {
          Color.clear
            .onAppear {
              if !ProactiveAssistantsPlugin.shared.isMonitoring {
                ProactiveAssistantsPlugin.shared.startMonitoring { _, _ in }
              }
              if let onComplete = onComplete {
                onComplete()
              }
            }
        } else {
          onboardingContent
        }
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .onAppear {
      Task {
        await graphViewModel.addGraphFromStorage()
        if graphViewModel.isEmpty {
          await graphViewModel.loadGraph()
        }
      }
    }
  }

  private var onboardingContent: some View {
    Group {
      if currentStep == 0 {
        LocalOnboardingIntroStepView(
          coordinator: OnboardingPagedIntroCoordinator(),
          graphViewModel: graphViewModel,
          stepIndex: 0,
          totalSteps: localStepCount,
          onContinue: { currentStep = 1 },
          onForceComplete: handleOnboardingComplete
        )
      } else if currentStep == 1 {
        LocalOnboardingPermissionsStepView(
          appState: appState,
          graphViewModel: graphViewModel,
          stepIndex: 1,
          totalSteps: localStepCount,
          onContinue: { currentStep = 2 },
          onSkip: { currentStep = 2 },
          onForceComplete: handleOnboardingComplete
        )
      } else {
        LocalOnboardingCompleteStepView(
          graphViewModel: graphViewModel,
          stepIndex: 2,
          totalSteps: localStepCount,
          onComplete: handleOnboardingComplete
        )
      }
    }
  }

  private func handleOnboardingComplete() {
    log("LocalOnboardingView: Onboarding complete")
    AnalyticsManager.shared.onboardingCompleted()

    chatProvider.stopAgent()

    UserDefaults.standard.set(true, forKey: "onboardingJustCompleted")
    UserDefaults.standard.set(true, forKey: "hasCompletedFileIndexing")
    PostOnboardingPromptSuggestions.save(
      OnboardingPromptSuggestionBuilder.build(from: OnboardingPagedIntroCoordinator()))

    chatProvider.isOnboarding = false
    OnboardingChatPersistence.clear()

    if let onComplete = onComplete {
      onComplete()
    }

    DispatchQueue.main.async {
      appState.hasCompletedOnboarding = true
    }

    Task {
      await GoalGenerationService.shared.generateNow()
    }
  }
}

// MARK: - Local Onboarding Permissions Step

struct LocalOnboardingPermissionsStepView: View {
  @ObservedObject var appState: AppState
  @ObservedObject var graphViewModel: MemoryGraphViewModel
  let stepIndex: Int
  let totalSteps: Int
  let onContinue: () -> Void
  let onSkip: (() -> Void)?
  let onForceComplete: (() -> Void)?

  @State private var permissionsGranted = false

  var body: some View {
    OnboardingStepScaffold(
      graphViewModel: graphViewModel,
      stepIndex: stepIndex,
      totalSteps: totalSteps,
      eyebrow: "Permissions",
      title: "Grant permissions to get started",
      description: "Omi needs a few permissions to capture your screen and understand your activity.",
      layoutMode: .split,
      rightPaneMode: .message(
        title: "Permissions",
        detail: "You can adjust these any time in Settings."
      ),
      showsSkip: true,
      onSkip: onSkip,
      onForceComplete: onForceComplete
    ) {
      VStack(alignment: .leading, spacing: 20) {
        permissionRow(
          icon: "display.and.arrow.down",
          title: "Screen Recording",
          description: "Let Omi see your screen",
          isGranted: appState.hasScreenRecordingPermission,
          action: { appState.requestScreenCapturePermission() }
        )

        permissionRow(
          icon: "mic.fill",
          title: "Microphone",
          description: "Transcribe meetings and voice notes",
          isGranted: appState.hasMicrophonePermission,
          action: { appState.requestMicrophonePermission() }
        )

        permissionRow(
          icon: "figure.wave",
          title: "Accessibility",
          description: "Know which app you're using",
          isGranted: appState.hasAccessibilityPermission,
          action: { appState.requestAccessibilityPermission() }
        )

        Spacer()

        Button("Continue") {
          onContinue()
        }
        .buttonStyle(OnboardingCardButtonStyle(isPrimary: true))
        .frame(maxWidth: .infinity, alignment: .center)
      }
    }
  }

  private func permissionRow(
    icon: String,
    title: String,
    description: String,
    isGranted: Bool,
    action: @escaping () -> Void
  ) -> some View {
    HStack(spacing: 16) {
      Image(systemName: icon)
        .font(.system(size: 18, weight: .semibold))
        .foregroundColor(isGranted ? OmiColors.success : OmiColors.purplePrimary)
        .frame(width: 28)

      VStack(alignment: .leading, spacing: 2) {
        Text(title)
          .font(.system(size: 15, weight: .medium))
          .foregroundColor(OmiColors.textPrimary)

        Text(description)
          .font(.system(size: 12))
          .foregroundColor(OmiColors.textTertiary)
      }

      Spacer()

      if isGranted {
        Image(systemName: "checkmark.circle.fill")
          .foregroundColor(OmiColors.success)
          .font(.system(size: 20))
      } else {
        Button("Grant") {
          action()
        }
        .font(.system(size: 13, weight: .semibold))
        .foregroundColor(.black)
        .padding(.horizontal, 14)
        .padding(.vertical, 6)
        .background(Color.white)
        .clipShape(RoundedRectangle(cornerRadius: 8))
      }
    }
    .padding(16)
    .background(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .fill(OmiColors.backgroundSecondary)
        .overlay(
          RoundedRectangle(cornerRadius: 12, style: .continuous)
            .stroke(Color.white.opacity(0.08), lineWidth: 1)
        )
    )
  }
}

// MARK: - Local Onboarding Complete Step

struct LocalOnboardingCompleteStepView: View {
  @ObservedObject var graphViewModel: MemoryGraphViewModel
  let stepIndex: Int
  let totalSteps: Int
  let onComplete: () -> Void

  var body: some View {
    OnboardingStepScaffold(
      graphViewModel: graphViewModel,
      stepIndex: stepIndex,
      totalSteps: totalSteps,
      eyebrow: "All Set",
      title: "You're ready to go",
      description: "Omi is running locally on your Mac. Everything stays private.",
      layoutMode: .split,
      rightPaneMode: .graph,
      rightPaneFooterText: "Your second brain is ready.",
      onForceComplete: nil
    ) {
      VStack(spacing: 20) {
        localStatusCard

        Button("Open Omi") {
          onComplete()
        }
        .buttonStyle(OnboardingCardButtonStyle(isPrimary: true))
      }
    }
  }

  private var localStatusCard: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack(spacing: 12) {
        Image(systemName: "checkmark.circle.fill")
          .font(.system(size: 24))
          .foregroundColor(OmiColors.success)

        VStack(alignment: .leading, spacing: 4) {
          Text("Local Mode Active")
            .font(.system(size: 16, weight: .semibold))
            .foregroundColor(OmiColors.textPrimary)

          Text("Your data never leaves this Mac")
            .font(.system(size: 13))
            .foregroundColor(OmiColors.textTertiary)
        }
      }

      Divider()
        .overlay(OmiColors.backgroundQuaternary)

      VStack(alignment: .leading, spacing: 10) {
        localStatusRow(icon: "person.crop.circle.fill", text: "Local account")
        localStatusRow(icon: "externaldrive.fill", text: "Storage on this Mac")
        localStatusRow(icon: "cloud.slash.fill", text: "No cloud sync")
      }
    }
    .padding(20)
    .frame(maxWidth: 400, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 16, style: .continuous)
        .fill(OmiColors.backgroundSecondary)
        .overlay(
          RoundedRectangle(cornerRadius: 16, style: .continuous)
            .stroke(Color.white.opacity(0.08), lineWidth: 1)
        )
    )
  }

  private func localStatusRow(icon: String, text: String) -> some View {
    HStack(spacing: 10) {
      Image(systemName: icon)
        .font(.system(size: 13))
        .foregroundColor(OmiColors.textTertiary)
        .frame(width: 18)

      Text(text)
        .font(.system(size: 13))
        .foregroundColor(OmiColors.textSecondary)
    }
  }
}
