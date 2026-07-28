import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:keryx_console/features/sessions/session_run_controller.dart';

/// Composer law: Active refuses silent second start; empty Send refused.
void main() {
  test('SessionRunState.hasActive true only for active status', () {
    const idle = SessionRunState();
    expect(idle.hasActive, isFalse);

    const active = SessionRunState(
      boundSessionId: 's1',
      activeRun: RunRecord(
        id: 'r1',
        sessionId: 's1',
        principalId: 'p',
        goal: 'g',
        status: 'active',
        origin: 'control_plane',
      ),
    );
    expect(active.hasActive, isTrue);

    const done = SessionRunState(
      boundSessionId: 's1',
      activeRun: RunRecord(
        id: 'r1',
        sessionId: 's1',
        principalId: 'p',
        goal: 'g',
        status: 'completed',
        origin: 'control_plane',
      ),
    );
    expect(done.hasActive, isFalse);
  });

  test('startRun refuses empty message', () async {
    final fake = _GuardOnlyRunController();
    await fake.startRun('s1', '   ');
    expect(fake.state.error, 'Message must not be empty');
    expect(fake.state.hasActive, isFalse);
  });

  test('startRun refuses when Active already present', () async {
    final fake = _GuardOnlyRunController(
      initial: const SessionRunState(
        boundSessionId: 's1',
        activeRun: RunRecord(
          id: 'r1',
          sessionId: 's1',
          principalId: 'p',
          goal: 'already',
          status: 'active',
          origin: 'control_plane',
        ),
      ),
    );
    await fake.startRun('s1', 'second goal');
    expect(fake.state.error, contains('Active root Run'));
    expect(fake.state.activeRun?.id, 'r1');
  });

  test('cancel refuses Run belonging to another Session', () async {
    final fake = _GuardOnlyRunController(
      initial: const SessionRunState(
        boundSessionId: 's-b',
        activeRun: RunRecord(
          id: 'r-a',
          sessionId: 's-a',
          principalId: 'p',
          goal: 'from A',
          status: 'active',
          origin: 'control_plane',
        ),
      ),
    );
    await fake.cancel();
    expect(fake.state.error, contains('another Session'));
  });
}

/// Test double that only exercises guards (no network).
class _GuardOnlyRunController extends StateNotifier<SessionRunState> {
  _GuardOnlyRunController({SessionRunState initial = const SessionRunState()})
      : super(initial);

  Future<void> startRun(String sessionId, String goal) async {
    if (state.hasActive && state.activeRun?.sessionId == sessionId) {
      state = state.copyWith(
        error:
            'Session already has an Active root Run — cancel or cancel-and-rerun',
      );
      return;
    }
    if (goal.trim().isEmpty) {
      state = state.copyWith(error: 'Message must not be empty');
      return;
    }
  }

  Future<void> cancel() async {
    final run = state.activeRun;
    final sessionId = state.boundSessionId;
    if (run == null || sessionId == null) return;
    if (run.sessionId != sessionId) {
      state = state.copyWith(
        error: 'Cannot cancel a Run belonging to another Session',
      );
    }
  }
}
