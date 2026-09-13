import XCTest
@testable import LocalBar

final class InstancePhaseTests: XCTestCase {

    // MARK: - iconState

    func test_iconState_running() {
        XCTAssertEqual(InstancePhase.running.iconState, .on)
    }

    func test_iconState_stopped_neverStarted() {
        XCTAssertEqual(InstancePhase.stopped(.neverStarted).iconState, .off)
    }

    func test_iconState_stopped_userStopped() {
        XCTAssertEqual(InstancePhase.stopped(.userStopped).iconState, .off)
    }

    func test_iconState_stopped_exitedCleanly() {
        XCTAssertEqual(InstancePhase.stopped(.exitedCleanly).iconState, .off)
    }

    func test_iconState_error() {
        let err = InstanceError(kind: .launchFailed, message: "x")
        XCTAssertEqual(InstancePhase.error(err).iconState, .error)
    }

    func test_iconState_starting() {
        XCTAssertEqual(InstancePhase.starting.iconState, .transitioning)
    }

    func test_iconState_stopping() {
        XCTAssertEqual(InstancePhase.stopping.iconState, .transitioning)
    }

    func test_iconState_switchingModel() {
        XCTAssertEqual(InstancePhase.switchingModel.iconState, .transitioning)
    }

    // MARK: - isTransitioning

    func test_isTransitioning_starting() {
        XCTAssertTrue(InstancePhase.starting.isTransitioning)
    }

    func test_isTransitioning_stopping() {
        XCTAssertTrue(InstancePhase.stopping.isTransitioning)
    }

    func test_isTransitioning_switchingModel() {
        XCTAssertTrue(InstancePhase.switchingModel.isTransitioning)
    }

    func test_isTransitioning_running() {
        XCTAssertFalse(InstancePhase.running.isTransitioning)
    }

    func test_isTransitioning_stopped() {
        XCTAssertFalse(InstancePhase.stopped(.neverStarted).isTransitioning)
    }

    func test_isTransitioning_error() {
        let err = InstanceError(kind: .launchFailed, message: "x")
        XCTAssertFalse(InstancePhase.error(err).isTransitioning)
    }

    // MARK: - isRunning

    func test_isRunning_running() {
        XCTAssertTrue(InstancePhase.running.isRunning)
    }

    func test_isRunning_notRunning() {
        let err = InstanceError(kind: .healthCheckFailed, message: "x")
        XCTAssertFalse(InstancePhase.stopped(.neverStarted).isRunning)
        XCTAssertFalse(InstancePhase.starting.isRunning)
        XCTAssertFalse(InstancePhase.stopping.isRunning)
        XCTAssertFalse(InstancePhase.switchingModel.isRunning)
        XCTAssertFalse(InstancePhase.error(err).isRunning)
    }

    // MARK: - error accessor

    func test_error_accessor_returnsError() {
        let err = InstanceError(kind: .crashed(exitCode: 1), message: "oops")
        XCTAssertEqual(InstancePhase.error(err).error, err)
    }

    func test_error_accessor_nilWhenNotError() {
        XCTAssertNil(InstancePhase.running.error)
        XCTAssertNil(InstancePhase.starting.error)
        XCTAssertNil(InstancePhase.stopped(.userStopped).error)
    }

    // MARK: - aggregateIconState

    func test_aggregate_empty_returnsOff() {
        let phases: [InstancePhase] = []
        XCTAssertEqual(phases.aggregateIconState, .off)
    }

    func test_aggregate_allStopped_returnsOff() {
        let phases: [InstancePhase] = [.stopped(.neverStarted), .stopped(.userStopped)]
        XCTAssertEqual(phases.aggregateIconState, .off)
    }

    func test_aggregate_oneRunning_returnsOn() {
        let phases: [InstancePhase] = [.stopped(.neverStarted), .running]
        XCTAssertEqual(phases.aggregateIconState, .on)
    }

    func test_aggregate_transitioning_beatsOn() {
        let phases: [InstancePhase] = [.running, .starting]
        XCTAssertEqual(phases.aggregateIconState, .transitioning)
    }

    func test_aggregate_error_beatsTransitioning() {
        let err = InstanceError(kind: .launchFailed, message: "x")
        let phases: [InstancePhase] = [.starting, .error(err)]
        XCTAssertEqual(phases.aggregateIconState, .error)
    }

    func test_aggregate_error_beatsRunningAndTransitioning() {
        let err = InstanceError(kind: .portConflict(port: 11434, occupiedBy: "x"), message: "x")
        let phases: [InstancePhase] = [.running, .starting, .error(err)]
        XCTAssertEqual(phases.aggregateIconState, .error)
    }

    func test_aggregate_singleSwitchingModel_returnsTransitioning() {
        XCTAssertEqual([InstancePhase.switchingModel].aggregateIconState, .transitioning)
    }

    // MARK: - isStopped

    func test_isStopped_stoppedNeverStarted() {
        XCTAssertTrue(InstancePhase.stopped(.neverStarted).isStopped)
    }

    func test_isStopped_running() {
        XCTAssertFalse(InstancePhase.running.isStopped)
    }

    func test_isStopped_starting() {
        XCTAssertFalse(InstancePhase.starting.isStopped)
    }

    // MARK: - isError

    func test_isError_error() {
        let err = InstanceError(kind: .launchFailed, message: "x")
        XCTAssertTrue(InstancePhase.error(err).isError)
    }

    func test_isError_stopped() {
        XCTAssertFalse(InstancePhase.stopped(.neverStarted).isError)
    }

    func test_isError_running() {
        XCTAssertFalse(InstancePhase.running.isError)
    }

    // MARK: - isStoppable

    func test_isStoppable_running() {
        XCTAssertTrue(InstancePhase.running.isStoppable)
    }

    func test_isStoppable_starting() {
        XCTAssertTrue(InstancePhase.starting.isStoppable)
    }

    func test_isStoppable_stopped() {
        XCTAssertFalse(InstancePhase.stopped(.neverStarted).isStoppable)
    }

    func test_isStoppable_stopping() {
        XCTAssertFalse(InstancePhase.stopping.isStoppable)
    }

    func test_isStoppable_switchingModel() {
        XCTAssertFalse(InstancePhase.switchingModel.isStoppable)
    }

    func test_isStoppable_error() {
        let err = InstanceError(kind: .launchFailed, message: "x")
        XCTAssertFalse(InstancePhase.error(err).isStoppable)
    }
}
