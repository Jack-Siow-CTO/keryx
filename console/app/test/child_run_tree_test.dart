import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:keryx_console/features/inbox/inbox_screen.dart';
import 'package:keryx_console/features/sessions/session_detail.dart';
import 'package:keryx_console/features/sessions/session_run_controller.dart';
import 'package:keryx_console/features/sessions/sessions_controller.dart';
import 'package:keryx_console/features/sessions/sessions_list.dart';
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

RunRecord _root({
  String id = 'r1',
  String status = 'active',
  String goal = 'parent goal',
  String? parentRunId,
}) {
  return RunRecord(
    id: id,
    sessionId: 's1',
    principalId: 'p1',
    goal: goal,
    status: status,
    origin: 'control_plane',
    parentRunId: parentRunId,
  );
}

void main() {
  group('projectChildRunTree', () {
    test('nests Child Runs under Active root from live linkage', () {
      var live = const <LiveActivityItem>[];
      live = applyLiveActivity(
        live,
        _event(
          type: 'child_run_started',
          payload: {'child_run_id': 'c1', 'goal': 'scan workspace'},
        ),
      )!;
      live = applyLiveActivity(
        live,
        _event(
          type: 'child_run_started',
          seq: 2,
          payload: {'child_run_id': 'c2', 'goal': 'summarize'},
        ),
      )!;

      final tree = projectChildRunTree(
        activeRun: _root(),
        liveActivity: live,
      );

      expect(tree, isNotNull);
      expect(tree!.rootRunId, 'r1');
      expect(tree.rootGoal, 'parent goal');
      expect(tree.rootStatus, 'active');
      expect(tree.hasChildren, isTrue);
      expect(tree.children, hasLength(2));
      expect(tree.children[0].childRunId, 'c1');
      expect(tree.children[0].goal, 'scan workspace');
      expect(tree.children[0].status, 'running');
      expect(tree.children[1].childRunId, 'c2');
      expect(tree.anyChildRunning, isTrue);
    });

    test('null when no children and when Run is itself a Child', () {
      expect(
        projectChildRunTree(activeRun: _root(), liveActivity: const []),
        isNull,
      );
      expect(projectChildRunTree(activeRun: null, liveActivity: const []), isNull);

      final live = applyLiveActivity(
        const [],
        _event(
          type: 'child_run_started',
          payload: {'child_run_id': 'c1', 'goal': 'x'},
        ),
      )!;
      // A Child Run record must never project as the Session root contact.
      expect(
        projectChildRunTree(
          activeRun: _root(parentRunId: 'parent-root'),
          liveActivity: live,
        ),
        isNull,
      );
    });

    test('SessionRunState.childRunTree exposes projection', () {
      final live = applyLiveActivity(
        const [],
        _event(
          type: 'child_run_started',
          payload: {'child_run_id': 'c1', 'goal': 'delegate'},
        ),
      )!;
      final state = SessionRunState(
        boundSessionId: 's1',
        activeRun: _root(),
        liveActivity: live,
      );
      expect(state.childRunTree?.children.single.childRunId, 'c1');
    });
  });

  group('parent stop cascade', () {
    test('run_cancelled marks running Child Runs cancelled', () {
      var live = applyLiveActivity(
        const [],
        _event(
          type: 'child_run_started',
          payload: {'child_run_id': 'c1', 'goal': 'work'},
        ),
      )!;
      live = applyLiveActivity(
        live,
        _event(
          type: 'child_run_started',
          seq: 2,
          payload: {'child_run_id': 'c2', 'goal': 'done later'},
        ),
      )!;
      // Finish one child before parent cancel.
      live = applyLiveActivity(
        live,
        _event(
          type: 'child_run_finished',
          seq: 3,
          payload: {'child_run_id': 'c2', 'status': 'completed'},
        ),
      )!;

      live = applyLiveActivity(
        live,
        _event(type: 'run_cancelled', seq: 9),
      )!;

      final running = live.where((e) => e.kind == LiveActivityKind.childRun);
      expect(
        running.firstWhere((e) => e.id == 'child:c1').status,
        'cancelled',
      );
      expect(
        running.firstWhere((e) => e.id == 'child:c2').status,
        'completed',
        reason: 'already terminal children keep own status',
      );

      final tree = projectChildRunTree(
        activeRun: _root(status: 'cancelled'),
        liveActivity: live,
      )!;
      expect(tree.rootStatus, 'cancelled');
      expect(tree.children.firstWhere((c) => c.childRunId == 'c1').status,
          'cancelled');
    });

    test('markRunningChildrenStopped is pure and status-only', () {
      const live = [
        LiveActivityItem(
          id: 'child:c1',
          kind: LiveActivityKind.childRun,
          title: 'scan',
          status: 'running',
          summary: 'scan',
        ),
        LiveActivityItem(
          id: 'tool:x',
          kind: LiveActivityKind.tool,
          title: 'x',
          status: 'running',
          summary: 'Tool started',
        ),
      ];
      final next = markRunningChildrenStopped(live, 'cancelled');
      expect(next[0].status, 'cancelled');
      expect(next[0].summary, 'scan');
      expect(next[1].status, 'running', reason: 'tools are not Child Runs');
    });
  });

  group('ChildRunTreeStrip widget', () {
    testWidgets('shows root and nested children with statuses', (tester) async {
      const tree = ChildRunTree(
        rootRunId: 'r1',
        rootGoal: 'parent goal',
        rootStatus: 'active',
        children: [
          ChildRunTreeNode(
            childRunId: 'c1',
            goal: 'scan workspace',
            status: 'running',
          ),
          ChildRunTreeNode(
            childRunId: 'c2',
            goal: 'summarize',
            status: 'completed',
          ),
        ],
      );

      await tester.pumpWidget(
        MaterialApp(
          theme: KeryxTheme.light(),
          home: const Scaffold(body: ChildRunTreeStrip(tree: tree)),
        ),
      );

      expect(find.textContaining('Root Run · active'), findsOneWidget);
      expect(find.text('parent goal'), findsOneWidget);
      expect(find.text('scan workspace'), findsOneWidget);
      expect(find.text('summarize'), findsOneWidget);
      expect(find.text('running'), findsOneWidget);
      expect(find.text('completed'), findsOneWidget);
      // Nested under parent — tree glyph present.
      expect(find.byIcon(Icons.account_tree_outlined), findsOneWidget);
    });

    testWidgets('parent stop is visible after cancel cascade', (tester) async {
      const tree = ChildRunTree(
        rootRunId: 'r1',
        rootGoal: 'parent goal',
        rootStatus: 'cancelled',
        children: [
          ChildRunTreeNode(
            childRunId: 'c1',
            goal: 'scan workspace',
            status: 'cancelled',
          ),
        ],
      );

      await tester.pumpWidget(
        MaterialApp(
          theme: KeryxTheme.light(),
          home: const Scaffold(body: ChildRunTreeStrip(tree: tree)),
        ),
      );

      expect(find.textContaining('Root Run · cancelled'), findsOneWidget);
      expect(find.text('parent stop'), findsOneWidget);
      expect(find.text('cancelled'), findsWidgets);
      expect(
        find.textContaining('Cancel stops the root and Child Runs together'),
        findsOneWidget,
      );
    });
  });

  group('Child Runs are not Session contacts', () {
    testWidgets('chat list only shows Session rows, never Child Run rows',
        (tester) async {
      final session = SessionSummary(
        id: 's1',
        principalId: 'p1',
        title: 'Demo chat',
        titleIsCustom: true,
        createdAt: 1,
        updatedAt: 1,
        lastMessagePreview: 'hello',
        activeRootRun: const ActiveRootRunSummary(
          id: 'r1',
          goal: 'working with children',
          origin: 'control_plane',
          status: 'active',
        ),
        pendingApprovalCount: 0,
      );

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            sessionsControllerProvider.overrideWith(
              (ref) => _FakeSessions(
                ref,
                SessionsState(sessions: [session], selectedId: session.id),
              ),
            ),
            inboxProvider.overrideWith((ref) async => const <InboxItem>[]),
          ],
          child: MaterialApp(
            theme: KeryxTheme.light(),
            home: Scaffold(
              body: ChatListPane(
                onSelectSession: (_) {},
                onSelectNeedsYou: () {},
                onNewChat: (_) {},
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // One Session contact only.
      expect(find.text('Demo chat'), findsOneWidget);
      expect(find.text('SESSIONS'), findsOneWidget);
      // No invented Child Run contacts in the chat list.
      expect(find.textContaining('Child Run'), findsNothing);
      expect(find.text('scan workspace'), findsNothing);
    });
  });
}

class _FakeSessions extends SessionsController {
  _FakeSessions(super.ref, SessionsState initial) {
    state = initial;
  }

  @override
  Future<void> refresh() async {}
}
