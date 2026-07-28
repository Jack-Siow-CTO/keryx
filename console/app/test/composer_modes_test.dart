import 'package:flutter_test/flutter_test.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:keryx_console/features/sessions/session_run_controller.dart';

/// Composer law: Active refuses silent second start (ticket #42).
void main() {
  test('SessionRunState.hasActive true only for active status', () {
    const idle = SessionRunState();
    expect(idle.hasActive, isFalse);

    const active = SessionRunState(
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
}
