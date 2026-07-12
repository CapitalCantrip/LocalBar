import Foundation

// MARK: - Instance phase (state machine)

enum InstancePhase: Equatable, Sendable {
    case stopped(StopReason)
    case starting
    case running
    case stopping
    case switchingModel
    case error(InstanceError)

    enum StopReason: Equatable, Sendable {
        case neverStarted
        case userStopped
        case exitedCleanly
    }

    // MARK: Derived icon state

    /// Maps the full phase set to the four icon states from the design.
    var iconState: IconState {
        switch self {
        case .running:                    return .on
        case .stopped:                    return .off
        case .error:                      return .error
        case .starting, .stopping, .switchingModel: return .transitioning
        }
    }

    var isTransitioning: Bool {
        switch self {
        case .starting, .stopping, .switchingModel: return true
        default: return false
        }
    }

    var isRunning: Bool {
        if case .running = self { return true }
        return false
    }

    var error: InstanceError? {
        if case .error(let e) = self { return e }
        return nil
    }
}

// MARK: - Icon state

/// The four menu bar icon states from the architecture doc.
enum IconState: Sendable, Equatable {
    case on
    case off
    case error
    case transitioning
}

// MARK: - Instance error

struct InstanceError: Equatable, Sendable {
    enum Kind: Equatable, Sendable {
        case launchFailed
        case crashed(exitCode: Int32?)
        case healthCheckFailed
        case portConflict(port: Int, occupiedBy: String?)
        case shutdownTimedOut
    }

    var kind: Kind
    var message: String
    var occurredAt: Date

    /// D4: keys for the one-click rollback action in the error UI.
    var lastAttemptedModelKey: String?
    var previousModelKey: String?

    init(kind: Kind, message: String, lastAttemptedModelKey: String? = nil, previousModelKey: String? = nil) {
        self.kind = kind
        self.message = message
        self.occurredAt = Date()
        self.lastAttemptedModelKey = lastAttemptedModelKey
        self.previousModelKey = previousModelKey
    }
}

// MARK: - Aggregate icon state (InstanceRegistry)

extension [InstancePhase] {
    /// Fold over all controller phases to derive the single menu bar icon state.
    /// Priority: error > transitioning > on > off.
    var aggregateIconState: IconState {
        if contains(where: { $0.iconState == .error })        { return .error }
        if contains(where: { $0.iconState == .transitioning }) { return .transitioning }
        if contains(where: { $0.iconState == .on })            { return .on }
        return .off
    }
}
