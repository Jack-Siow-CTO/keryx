import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:keryx_console/features/inbox/inbox_screen.dart';
import 'package:keryx_console/features/sessions/composer.dart';
import 'package:keryx_console/features/sessions/session_detail.dart';
import 'package:keryx_console/features/sessions/session_run_controller.dart';
import 'package:keryx_console/features/sessions/sessions_controller.dart';
import 'package:keryx_console/features/sessions/sticky_approval.dart';
import 'package:keryx_console/features/sessions/transcript_pane.dart';
import 'package:keryx_console/features/shell/messaging_shell.dart';
import 'package:keryx_console/theme/keryx_theme.dart';

SessionSummary _session({
  String id = 's1',
  String title = 'Demo chat',
  int pending = 0,
  ActiveRootRunSummary? active,
}) {
  return SessionSummary(
    id: id,
    principalId: 'p1',
    title: title,
    titleIsCustom: true,
    createdAt: 1,
    updatedAt: 1,
    lastMessagePreview: 'hello preview',
    activeRootRun: active,
    pendingApprovalCount: pending,
  );
}

/// Minimal fake sessions controller for widget tests.
class FakeSessionsController extends SessionsController {
  FakeSessionsController(super.ref, SessionsState initial) {
    state = initial;
  }

  @override
  Future<void> refresh() async {}

  @override
  Future<void> open(String sessionId) async {
    state = state.copyWith(selectedId: sessionId);
  }

  @override
  Future<SessionSummary?> createSession() async {
    final created = _session(id: 'new-1', title: 'New Session');
    state = state.copyWith(
      sessions: [created, ...state.sessions],
      selectedId: created.id,
    );
    return created;
  }
}

class FakeSessionRunController extends SessionRunController {
  FakeSessionRunController(super.ref, SessionRunState initial) {
    state = initial;
  }

  @override
  Future<void> syncFromSession(SessionSummary? session) async {}

  @override
  Future<void> reconnect() async {}

  @override
  Future<void> startRun(
    String sessionId,
    String goal, {
    String? provider,
    String? model,
  }) async {
    if (state.hasActive) {
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
    state = state.copyWith(
      boundSessionId: sessionId,
      activeRun: RunRecord(
        id: 'r-new',
        sessionId: sessionId,
        principalId: 'p1',
        goal: goal.trim(),
        status: 'active',
        origin: 'control_plane',
      ),
    );
  }
}

Widget _wrap(Widget child, List<Override> overrides) {
  return ProviderScope(
    overrides: overrides,
    child: MaterialApp(
      theme: KeryxTheme.light(),
      home: Scaffold(body: child),
    ),
  );
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('composer Send-first', () {
    testWidgets('idle primary control is Send not Start Run', (tester) async {
      final session = _session();
      await tester.pumpWidget(
        _wrap(
          const SessionComposer(),
          [
            sessionsControllerProvider.overrideWith(
              (ref) => FakeSessionsController(
                ref,
                SessionsState(sessions: [session], selectedId: session.id),
              ),
            ),
            sessionRunControllerProvider.overrideWith(
              (ref) => FakeSessionRunController(ref, const SessionRunState()),
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Send'), findsOneWidget);
      expect(find.text('Start Run'), findsNothing);
      expect(find.textContaining('Message the agent'), findsOneWidget);
    });

    testWidgets('Active exposes Cancel and refuses second start path',
        (tester) async {
      final session = _session(
        active: const ActiveRootRunSummary(
          id: 'r1',
          goal: 'working',
          origin: 'control_plane',
          status: 'active',
        ),
      );
      final activeState = SessionRunState(
        activeRun: RunRecord(
          id: 'r1',
          sessionId: session.id,
          principalId: 'p1',
          goal: 'working',
          status: 'active',
          origin: 'control_plane',
        ),
      );

      await tester.pumpWidget(
        _wrap(
          const SessionComposer(),
          [
            sessionsControllerProvider.overrideWith(
              (ref) => FakeSessionsController(
                ref,
                SessionsState(sessions: [session], selectedId: session.id),
              ),
            ),
            sessionRunControllerProvider.overrideWith(
              (ref) => FakeSessionRunController(ref, activeState),
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Send'), findsNothing);
      expect(find.text('Cancel Run'), findsOneWidget);
      expect(find.text('Cancel & re-run'), findsOneWidget);
    });

    testWidgets('Session detail shows Child Run tree and Cancel on Active',
        (tester) async {
      // Active Run animates a progress strip — use pump, not pumpAndSettle.
      tester.view.physicalSize = const Size(1200, 900);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final session = _session(
        active: const ActiveRootRunSummary(
          id: 'r1',
          goal: 'parent goal',
          origin: 'control_plane',
          status: 'active',
        ),
      );
      final live = [
        const LiveActivityItem(
          id: 'child:c1',
          kind: LiveActivityKind.childRun,
          title: 'scan workspace',
          status: 'running',
          summary: 'scan workspace',
        ),
      ];
      final activeState = SessionRunState(
        boundSessionId: session.id,
        activeRun: RunRecord(
          id: 'r1',
          sessionId: session.id,
          principalId: 'p1',
          goal: 'parent goal',
          status: 'active',
          origin: 'control_plane',
        ),
        liveActivity: live,
      );

      await tester.pumpWidget(
        _wrap(
          const SessionDetailPane(),
          [
            sessionsControllerProvider.overrideWith(
              (ref) => FakeSessionsController(
                ref,
                SessionsState(sessions: [session], selectedId: session.id),
              ),
            ),
            sessionRunControllerProvider.overrideWith(
              (ref) => FakeSessionRunController(ref, activeState),
            ),
          ],
        ),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));

      expect(find.textContaining('Root Run · active'), findsOneWidget);
      expect(find.text('scan workspace'), findsWidgets);
      expect(find.text('Cancel Run'), findsOneWidget);
    });

    testWidgets('projection Active hides Send without waiting for hydrate',
        (tester) async {
      final session = _session(
        active: const ActiveRootRunSummary(
          id: 'r1',
          goal: 'working',
          origin: 'control_plane',
          status: 'active',
        ),
      );
      // Controller still idle (hydrate lag) — UI must use projection.
      await tester.pumpWidget(
        _wrap(
          const SessionComposer(),
          [
            sessionsControllerProvider.overrideWith(
              (ref) => FakeSessionsController(
                ref,
                SessionsState(sessions: [session], selectedId: session.id),
              ),
            ),
            sessionRunControllerProvider.overrideWith(
              (ref) => FakeSessionRunController(ref, const SessionRunState()),
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Send'), findsNothing);
      expect(find.text('Cancel Run'), findsOneWidget);
    });
  });

  group('New chat', () {
    test('createSession selects empty Session', () async {
      final container = ProviderContainer(
        overrides: [
          sessionsControllerProvider.overrideWith(
            (ref) => FakeSessionsController(ref, const SessionsState()),
          ),
        ],
      );
      addTearDown(container.dispose);

      final created =
          await container.read(sessionsControllerProvider.notifier).createSession();
      expect(created, isNotNull);
      final state = container.read(sessionsControllerProvider);
      expect(state.selectedId, 'new-1');
      expect(state.sessions.first.id, 'new-1');
    });
  });

  /// Messaging-day Approvals path (ADR 0033 / checklist line 1 Approvals half).
  ///
  /// Same pending Approval is sticky in open Session and under Needs you.
  group('messaging-day Approvals (sticky + Needs you)', () {
    final session = _session(pending: 1);
    final pendingItem = InboxItem(
      id: 'approval:a1',
      kind: 'approval_pending',
      sessionId: session.id,
      runId: 'r1',
      approvalId: 'a1',
      title: 'Allow exec',
      summary: 'run rm -rf /tmp/x',
      createdAt: 1,
    );

    testWidgets('sticky card shows Approve/Deny for open Session pending',
        (tester) async {
      await tester.pumpWidget(
        _wrap(
          const StickyApprovalCard(),
          [
            sessionsControllerProvider.overrideWith(
              (ref) => FakeSessionsController(
                ref,
                SessionsState(sessions: [session], selectedId: session.id),
              ),
            ),
            inboxProvider.overrideWith((ref) async => [pendingItem]),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Allow exec'), findsOneWidget);
      expect(find.text('run rm -rf /tmp/x'), findsOneWidget);
      expect(find.text('Approve'), findsOneWidget);
      expect(find.text('Deny'), findsOneWidget);
      expect(find.text('Approval'), findsOneWidget);
    });

    testWidgets('sticky card hides when pending belongs to another Session',
        (tester) async {
      const other = InboxItem(
        id: 'approval:a2',
        kind: 'approval_pending',
        sessionId: 'other-session',
        approvalId: 'a2',
        title: 'Other session Approval',
        summary: 'not this thread',
        createdAt: 1,
      );
      // Open session has no pending count and inbox item is for another Session.
      final idle = _session(pending: 0);

      await tester.pumpWidget(
        _wrap(
          const StickyApprovalCard(),
          [
            sessionsControllerProvider.overrideWith(
              (ref) => FakeSessionsController(
                ref,
                SessionsState(sessions: [idle], selectedId: idle.id),
              ),
            ),
            inboxProvider.overrideWith((ref) async => [other]),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Other session Approval'), findsNothing);
      expect(find.text('Approve'), findsNothing);
    });

    testWidgets('sticky shows count fallback when projection pending but Inbox lag',
        (tester) async {
      await tester.pumpWidget(
        _wrap(
          const StickyApprovalCard(),
          [
            sessionsControllerProvider.overrideWith(
              (ref) => FakeSessionsController(
                ref,
                SessionsState(sessions: [session], selectedId: session.id),
              ),
            ),
            inboxProvider.overrideWith((ref) async => const <InboxItem>[]),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('1 pending Approval'), findsOneWidget);
      expect(find.text('Approve'), findsNothing);
    });

    testWidgets('Needs you surfaces same pending Approval with actions',
        (tester) async {
      await tester.pumpWidget(
        _wrap(
          const NeedsYouPane(),
          [
            inboxProvider.overrideWith((ref) async => [pendingItem]),
            sessionsControllerProvider.overrideWith(
              (ref) => FakeSessionsController(ref, const SessionsState()),
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Needs you'), findsOneWidget);
      expect(find.text('Allow exec'), findsOneWidget);
      expect(find.text('run rm -rf /tmp/x'), findsOneWidget);
      expect(find.text('Approve'), findsOneWidget);
      expect(find.text('Deny'), findsOneWidget);
      expect(find.text('Open chat'), findsOneWidget);
    });

    test(
      'dual surface shares approval_id and session_id (contract golden)',
      () {
        // Same InboxItem feeds sticky filter and Needs you list — not two SoRs.
        expect(pendingItem.kind, 'approval_pending');
        expect(pendingItem.approvalId, 'a1');
        expect(pendingItem.sessionId, session.id);
        expect(session.pendingApprovalCount, 1);
      },
    );
  });

  group('layered timeline', () {
    testWidgets('prose and tool activity render distinctly', (tester) async {
      // Presentation-level: build message widgets via TranscriptMessage models.
      const prose = TranscriptMessage(
        id: 'm1',
        role: 'assistant',
        content: 'Here is the answer',
        createdAt: 100,
      );
      const tool = TranscriptMessage(
        id: 'm2',
        role: 'tool',
        content: 'done',
        createdAt: 101,
        tool: ToolCompact(
          name: 'workspace_read',
          status: 'ok',
          summary: 'read file',
          artifactRefs: [],
        ),
      );

      expect(prose.isTool, isFalse);
      expect(tool.isTool, isTrue);
      expect(tool.tool?.name, 'workspace_read');

      // Collapsible activity is first-class in the timeline (not status-strip only).
      await tester.pumpWidget(
        MaterialApp(
          theme: KeryxTheme.light(),
          home: Scaffold(
            body: ActivityBlock(
              blockId: tool.id,
              title: tool.tool!.name,
              status: tool.tool!.status,
              summary: tool.tool!.summary,
              looksLikeChild: false,
              expanded: false,
              onToggle: () {},
            ),
          ),
        ),
      );
      expect(find.text('workspace_read'), findsOneWidget);
      expect(find.text('ok'), findsOneWidget);
      expect(find.text('read file'), findsOneWidget);
    });
  });

  group('session detail empty teaching', () {
    testWidgets('empty selection teaches chat list / New chat', (tester) async {
      await tester.pumpWidget(
        _wrap(
          const SessionDetailPane(),
          [
            sessionsControllerProvider.overrideWith(
              (ref) => FakeSessionsController(ref, const SessionsState()),
            ),
            sessionRunControllerProvider.overrideWith(
              (ref) => FakeSessionRunController(ref, const SessionRunState()),
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Select a chat'), findsOneWidget);
      expect(find.textContaining('New chat'), findsOneWidget);
    });
  });

  group('breakpoints', () {
    test('wide and medium constants match design system', () {
      expect(MessagingShell.wideBreakpoint, 1100);
      expect(MessagingShell.mediumBreakpoint, 720);
    });
  });
}
