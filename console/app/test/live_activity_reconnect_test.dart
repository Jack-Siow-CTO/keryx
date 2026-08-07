import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:keryx_console/features/sessions/session_run_controller.dart';
import 'package:keryx_console/features/sessions/sessions_controller.dart';
import 'package:keryx_console/features/sessions/transcript_pane.dart';
import 'package:keryx_console/theme/keryx_theme.dart';

RunEvent _event({
  required String type,
  Map<String, dynamic>? payload,
  int seq = 1,
  String runId = 'r1',
}) {
  final kind = <String, dynamic>{'type': type};
  if (payload != null) kind['payload'] = payload;
  return RunEvent(
    runId: runId,
    seq: seq,
    type: type,
    raw: {
      'run_id': runId,
      'seq': seq,
      'kind': kind,
    },
    payload: payload ?? const {},
  );
}

void main() {
  group('applyLiveActivity depth', () {
    test('tool start/finish upserts one collapsible row (not flat dump)', () {
      var items = const <LiveActivityItem>[];

      final started = applyLiveActivity(
        items,
        _event(type: 'tool_started', payload: {'name': 'workspace_read'}),
      );
      expect(started, isNotNull);
      items = started!;
      expect(items, hasLength(1));
      expect(items.single.id, 'tool:workspace_read');
      expect(items.single.status, 'running');
      expect(items.single.kind, LiveActivityKind.tool);
      expect(items.single.stripLabel, contains('Tool · workspace_read'));

      final finished = applyLiveActivity(
        items,
        _event(
          type: 'tool_finished',
          seq: 2,
          payload: {'name': 'workspace_read'},
        ),
      );
      expect(finished, isNotNull);
      items = finished!;
      expect(items, hasLength(1), reason: 'finish must update in place');
      expect(items.single.status, 'finished');
    });

    test('Child Run start/finish is collapsible activity, not separate chat', () {
      var items = const <LiveActivityItem>[];
      final started = applyLiveActivity(
        items,
        _event(
          type: 'child_run_started',
          payload: {'child_run_id': 'c1', 'goal': 'scan workspace'},
        ),
      )!;
      items = started;
      expect(items.single.kind, LiveActivityKind.childRun);
      expect(items.single.looksLikeChild, isTrue);
      expect(items.single.summary, 'scan workspace');

      final finished = applyLiveActivity(
        items,
        _event(
          type: 'child_run_finished',
          seq: 2,
          payload: {'child_run_id': 'c1', 'status': 'completed'},
        ),
      )!;
      expect(finished, hasLength(1));
      expect(finished.single.status, 'completed');
    });

    test('model deltas never become activity rows', () {
      final delta = applyLiveActivity(
        const [],
        _event(type: 'model_delta', payload: {'text': 'hi'}),
      );
      expect(delta, isNull);

      final started = applyLiveActivity(
        const [],
        _event(type: 'model_started'),
      );
      expect(started, isNull);
    });

    test('approval and terminal map to status activity', () {
      var items = applyLiveActivity(
        const [],
        _event(
          type: 'approval_waiting',
          payload: {
            'action': 'terminal',
            'summary': 'Run host command',
          },
        ),
      )!;
      expect(items.single.kind, LiveActivityKind.status);
      expect(items.single.status, 'waiting');

      items = applyLiveActivity(
        items,
        _event(type: 'run_completed', seq: 9),
      )!;
      expect(items.any((e) => e.id == 'status:run'), isTrue);
      expect(
        items.firstWhere((e) => e.id == 'status:run').status,
        'completed',
      );
    });
  });

  group('SessionRunState reconnect epoch', () {
    test('copyWith and strip snippet track live activity', () {
      const item = LiveActivityItem(
        id: 'tool:x',
        kind: LiveActivityKind.tool,
        title: 'x',
        status: 'running',
        summary: 'Tool started',
      );
      const state = SessionRunState(
        boundSessionId: 's1',
        liveActivity: [item],
        reconnectEpoch: 2,
      );
      expect(state.lastActivitySnippet, 'Tool · x · running');
      expect(state.copyWith(reconnectEpoch: 3).reconnectEpoch, 3);
      expect(state.hasActive, isFalse);
    });
  });

  group('ActivityBlock widget', () {
    testWidgets('collapses summary until expanded', (tester) async {
      var expanded = false;
      await tester.pumpWidget(
        MaterialApp(
          theme: KeryxTheme.light(),
          home: Scaffold(
            body: StatefulBuilder(
              builder: (context, setState) {
                return ActivityBlock(
                  blockId: 'tool:workspace_read',
                  title: 'workspace_read',
                  status: 'running',
                  summary: 'reading /tmp/notes.md with a long body that truncates',
                  looksLikeChild: false,
                  expanded: expanded,
                  live: true,
                  onToggle: () => setState(() => expanded = !expanded),
                );
              },
            ),
          ),
        ),
      );

      expect(find.text('workspace_read'), findsOneWidget);
      expect(find.text('live'), findsOneWidget);
      expect(find.text('running'), findsOneWidget);
      // Collapsed: single-line summary still present as text.
      expect(find.textContaining('reading /tmp'), findsOneWidget);

      await tester.tap(find.text('workspace_read'));
      await tester.pumpAndSettle();
      expect(expanded, isTrue);
    });

    testWidgets('Child Run shows linkage note when expanded', (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: KeryxTheme.light(),
          home: const Scaffold(
            body: ActivityBlock(
              blockId: 'child:c1',
              title: 'Child Run',
              status: 'running',
              summary: 'scan workspace',
              looksLikeChild: true,
              expanded: true,
              live: true,
              onToggle: _noop,
            ),
          ),
        ),
      );

      expect(
        find.textContaining('Child Run (read-only linkage'),
        findsOneWidget,
      );
    });
  });

  group('reconnect controller surface', () {
    test('Fake reconnect bumps epoch for Transcript reload hook', () async {
      final container = ProviderContainer(
        overrides: [
          sessionsControllerProvider.overrideWith(
            (ref) => _IdleSessions(ref),
          ),
          sessionRunControllerProvider.overrideWith(
            (ref) => _RecordingRunController(ref),
          ),
        ],
      );
      addTearDown(container.dispose);

      final controller = container.read(sessionRunControllerProvider.notifier)
          as _RecordingRunController;
      expect(controller.state.reconnectEpoch, 0);
      await controller.reconnect();
      expect(controller.reconnectCalls, 1);
      expect(controller.state.reconnectEpoch, 1);
      expect(
        container.read(sessionRunControllerProvider).reconnectEpoch,
        1,
      );
    });
  });
}

void _noop() {}

class _IdleSessions extends SessionsController {
  _IdleSessions(super.ref);

  @override
  Future<void> refresh() async {}
}

class _RecordingRunController extends SessionRunController {
  _RecordingRunController(super.ref);

  int reconnectCalls = 0;

  @override
  Future<void> reconnect() async {
    reconnectCalls++;
    // Simulate successful reconnect without Worker: bump epoch only.
    state = state.copyWith(reconnectEpoch: state.reconnectEpoch + 1);
  }

  @override
  Future<void> syncFromSession(SessionSummary? session) async {}
}
