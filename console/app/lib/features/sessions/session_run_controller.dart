import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../auth/auth_controller.dart';
import '../skills/skill_load.dart';
import 'sessions_controller.dart';

/// Kind of collapsible live activity in the open thread (ADR 0015 / #75).
enum LiveActivityKind {
  tool,
  childRun,
  status,
}

/// One collapsible live activity row derived from Run events (not durable Transcript).
///
/// Upserted by [id] so tool start/finish and Child-Run pairs collapse in place —
/// never a flat dump of every SSE frame.
final class LiveActivityItem {
  const LiveActivityItem({
    required this.id,
    required this.kind,
    required this.title,
    required this.status,
    required this.summary,
  });

  final String id;
  final LiveActivityKind kind;
  final String title;
  final String status;
  final String summary;

  /// One-line status-strip label (human, never raw `type#seq`).
  String get stripLabel {
    switch (kind) {
      case LiveActivityKind.tool:
        // Skill load titles already start with "Skill · …" (#82).
        if (title.startsWith('Skill')) return '$title · $status';
        return 'Tool · $title · $status';
      case LiveActivityKind.childRun:
        final goalBit = summary.isNotEmpty && summary != title
            ? summary
            : (title.isNotEmpty && title != 'Child Run' ? title : '');
        if (goalBit.isEmpty) return 'Child Run · $status';
        return 'Child Run · $goalBit · $status';
      case LiveActivityKind.status:
        return summary.isNotEmpty ? summary : title;
    }
  }

  bool get looksLikeChild => kind == LiveActivityKind.childRun;

  LiveActivityItem copyWith({
    String? title,
    String? status,
    String? summary,
  }) {
    return LiveActivityItem(
      id: id,
      kind: kind,
      title: title ?? this.title,
      status: status ?? this.status,
      summary: summary ?? this.summary,
    );
  }
}

/// Apply one Run event into collapsible live activity (pure; unit-tested).
///
/// Returns null when the event should not appear as activity (model deltas).
List<LiveActivityItem>? applyLiveActivity(
  List<LiveActivityItem> current,
  RunEvent event,
) {
  if (event.isToolStarted) {
    final name = event.toolName ?? 'tool';
    if (isSkillLoadTool(name)) {
      final skill = skillNameFromLoadSignal(eventName: name);
      return _upsert(
        current,
        LiveActivityItem(
          id: 'tool:skill_load:${skill ?? 'unknown'}',
          kind: LiveActivityKind.tool,
          title: skillLoadActivityTitle(skill),
          status: 'running',
          summary: skill == null
              ? 'Skill load started'
              : 'Loading Skill package $skill',
        ),
      );
    }
    return _upsert(
      current,
      LiveActivityItem(
        id: 'tool:$name',
        kind: LiveActivityKind.tool,
        title: name,
        status: 'running',
        summary: 'Tool started',
      ),
    );
  }
  if (event.isToolFinished) {
    final name = event.toolName ?? 'tool';
    if (isSkillLoadTool(name)) {
      final skill = skillNameFromLoadSignal(eventName: name);
      final failed = name.toLowerCase().contains('error=');
      return _upsert(
        current,
        LiveActivityItem(
          id: 'tool:skill_load:${skill ?? 'unknown'}',
          kind: LiveActivityKind.tool,
          title: skillLoadActivityTitle(skill),
          status: failed ? 'error' : 'loaded',
          summary: failed
              ? 'Skill load failed'
              : (skill == null
                  ? 'Skill loaded'
                  : 'Loaded Skill package $skill'),
        ),
      );
    }
    return _upsert(
      current,
      LiveActivityItem(
        id: 'tool:$name',
        kind: LiveActivityKind.tool,
        title: name,
        status: 'finished',
        summary: 'Tool finished',
      ),
    );
  }
  if (event.isChildRunStarted) {
    final childId = event.childRunId ?? 'child';
    final goal = event.childRunGoal ?? '';
    return _upsert(
      current,
      LiveActivityItem(
        id: 'child:$childId',
        kind: LiveActivityKind.childRun,
        title: goal.isEmpty ? 'Child Run' : goal,
        status: 'running',
        summary: goal.isEmpty ? 'Child Run started' : goal,
      ),
    );
  }
  if (event.isChildRunFinished) {
    final childId = event.childRunId ?? 'child';
    final st = event.childRunTerminalStatus ?? 'finished';
    final existing = _find(current, 'child:$childId');
    return _upsert(
      current,
      LiveActivityItem(
        id: 'child:$childId',
        kind: LiveActivityKind.childRun,
        title: existing?.title ?? 'Child Run',
        status: st,
        summary: existing?.summary ?? 'Child Run $st',
      ),
    );
  }
  if (event.isApprovalWaiting) {
    final action = event.approvalAction ?? 'approval';
    final summary = event.approvalSummary ?? 'Waiting for Approval';
    return _upsert(
      current,
      LiveActivityItem(
        id: 'approval:$action',
        kind: LiveActivityKind.status,
        title: 'Approval',
        status: 'waiting',
        summary: summary,
      ),
    );
  }
  if (event.isApprovalResolved) {
    final decision = event.approvalDecision ?? 'resolved';
    final action = event.approvalAction ?? 'approval';
    return _upsert(
      current,
      LiveActivityItem(
        id: 'approval:$action',
        kind: LiveActivityKind.status,
        title: 'Approval',
        status: decision,
        summary: 'Approval $decision',
      ),
    );
  }

  final t = event.type.toLowerCase().replaceAll('.', '_');
  if (t.contains('delta') || t.contains('token') || t.contains('model')) {
    // model_started / model_finished / model_delta stay out of activity depth
    // (streaming text + status strip cover model progress).
    if (t.contains('model')) return null;
  }
  if (t.contains('budget')) {
    final msg = event.budgetMessage ?? 'Budget update';
    return _upsert(
      current,
      LiveActivityItem(
        id: 'status:budget',
        kind: LiveActivityKind.status,
        title: 'Budget',
        status: 'update',
        summary: msg,
      ),
    );
  }
  if (event.isTerminal) {
    String label;
    String status;
    if (t.contains('cancel')) {
      label = 'Run cancelled';
      status = 'cancelled';
    } else if (t.contains('fail')) {
      label = event.failureReason ?? 'Run failed';
      status = 'failed';
    } else {
      label = 'Run completed';
      status = 'completed';
    }
    var next = _upsert(
      current,
      LiveActivityItem(
        id: 'status:run',
        kind: LiveActivityKind.status,
        title: 'Run',
        status: status,
        summary: label,
      ),
    );
    // Parent stop bounds the tree: running Child Runs stop with the root.
    if (status == 'cancelled' || status == 'failed' || status == 'interrupted') {
      next = markRunningChildrenStopped(next, status);
    }
    return next;
  }
  return null;
}

/// Mark still-running Child Run activity as stopped with the parent (cancel cascade).
///
/// Worker cancels the tree; Console projection must not leave children "running"
/// after parent stop is known (SSE terminal or cancel REST response).
List<LiveActivityItem> markRunningChildrenStopped(
  List<LiveActivityItem> current,
  String terminalStatus,
) {
  return [
    for (final item in current)
      if (item.kind == LiveActivityKind.childRun && item.status == 'running')
        item.copyWith(status: terminalStatus)
      else
        item,
  ];
}

/// One Child Run under a root (read-only projection; not a Session contact).
final class ChildRunTreeNode {
  const ChildRunTreeNode({
    required this.childRunId,
    required this.goal,
    required this.status,
  });

  final String childRunId;
  final String goal;
  final String status;

  bool get isRunning => status == 'running';
  bool get isStopped =>
      status == 'cancelled' ||
      status == 'failed' ||
      status == 'interrupted' ||
      status == 'completed';
}

/// Root Run with nested Child Runs for open-Session projection (#83).
///
/// Built only from control-plane linkage (GET Run parent_run_id on the child
/// side + parent SSE child_run.*). Never invents separate Session contacts.
final class ChildRunTree {
  const ChildRunTree({
    required this.rootRunId,
    required this.rootGoal,
    required this.rootStatus,
    required this.children,
  });

  final String rootRunId;
  final String rootGoal;
  final String rootStatus;
  final List<ChildRunTreeNode> children;

  bool get hasChildren => children.isNotEmpty;

  bool get rootIsActive => rootStatus == 'active';

  bool get anyChildRunning => children.any((c) => c.isRunning);
}

/// Project Child Run tree under the Active **root** Run.
///
/// Returns null when there is no root, the Run is itself a Child
/// ([RunRecord.parentRunId] set), or no Child activity is known yet.
ChildRunTree? projectChildRunTree({
  required RunRecord? activeRun,
  required List<LiveActivityItem> liveActivity,
}) {
  if (activeRun == null) return null;
  // Child Runs must never project as the Session root contact.
  if (activeRun.parentRunId != null) return null;

  final children = <ChildRunTreeNode>[];
  for (final item in liveActivity) {
    if (item.kind != LiveActivityKind.childRun) continue;
    final id = item.id.startsWith('child:')
        ? item.id.substring('child:'.length)
        : item.id;
    children.add(
      ChildRunTreeNode(
        childRunId: id,
        goal: item.summary.isNotEmpty
            ? item.summary
            : (item.title.isNotEmpty ? item.title : 'Child Run'),
        status: item.status,
      ),
    );
  }
  if (children.isEmpty) return null;

  return ChildRunTree(
    rootRunId: activeRun.id,
    rootGoal: activeRun.goal,
    rootStatus: activeRun.status,
    children: children,
  );
}

LiveActivityItem? _find(List<LiveActivityItem> items, String id) {
  for (final i in items) {
    if (i.id == id) return i;
  }
  return null;
}

List<LiveActivityItem> _upsert(
  List<LiveActivityItem> current,
  LiveActivityItem next,
) {
  final out = <LiveActivityItem>[];
  var replaced = false;
  for (final i in current) {
    if (i.id == next.id) {
      out.add(next);
      replaced = true;
    } else {
      out.add(i);
    }
  }
  if (!replaced) out.add(next);
  // Cap depth so a long Run cannot grow unbounded presentation state.
  if (out.length > 32) {
    return out.sublist(out.length - 32);
  }
  return out;
}

/// Live Run + collapsible activity for the selected Session (tickets #41/#42, #59, #75).
///
/// Always bound to [boundSessionId]. Stale getRun / SSE results for a previous
/// Session are ignored so Cancel/Send never target the wrong Session.
final class SessionRunState {
  const SessionRunState({
    this.boundSessionId,
    this.activeRun,
    this.streamingText = '',
    this.liveActivity = const [],
    this.error,
    this.busy = false,
    this.reconnectEpoch = 0,
  });

  /// Session this controller state applies to (null when none selected).
  final String? boundSessionId;
  final RunRecord? activeRun;
  final String streamingText;

  /// Collapsible live activity (tool / Child-Run / status) for the open thread.
  final List<LiveActivityItem> liveActivity;
  final String? error;
  final bool busy;

  /// Bumps when Console reloads Run truth after resume / SSE loss (Transcript listens).
  final int reconnectEpoch;

  bool get hasActive => activeRun?.isActive == true;

  /// Last human activity line for the status strip (never raw event noise).
  String? get lastActivitySnippet =>
      liveActivity.isEmpty ? null : liveActivity.last.stripLabel;

  /// Child Runs nested under the Active root (null when none). Not contacts.
  ChildRunTree? get childRunTree => projectChildRunTree(
        activeRun: activeRun,
        liveActivity: liveActivity,
      );

  SessionRunState copyWith({
    String? boundSessionId,
    bool clearBoundSession = false,
    RunRecord? activeRun,
    bool clearRun = false,
    String? streamingText,
    List<LiveActivityItem>? liveActivity,
    String? error,
    bool clearError = false,
    bool? busy,
    int? reconnectEpoch,
  }) {
    return SessionRunState(
      boundSessionId:
          clearBoundSession ? null : (boundSessionId ?? this.boundSessionId),
      activeRun: clearRun ? null : (activeRun ?? this.activeRun),
      streamingText: streamingText ?? this.streamingText,
      liveActivity: liveActivity ?? this.liveActivity,
      error: clearError ? null : (error ?? this.error),
      busy: busy ?? this.busy,
      reconnectEpoch: reconnectEpoch ?? this.reconnectEpoch,
    );
  }
}

final sessionRunControllerProvider =
    StateNotifierProvider<SessionRunController, SessionRunState>((ref) {
  return SessionRunController(ref);
});

class SessionRunController extends StateNotifier<SessionRunState> {
  SessionRunController(this._ref) : super(const SessionRunState());

  final Ref _ref;
  StreamSubscription<RunEvent>? _sub;

  /// Monotonic token so late async results from a prior selection are dropped.
  int _syncGen = 0;

  /// Prevents concurrent reconnect storms from onError + onDone + lifecycle.
  bool _reconnectInFlight = false;

  KeryxApiClient? get _client =>
      _ref.read(authControllerProvider.notifier).client;

  String? get _selectedSessionId =>
      _ref.read(sessionsControllerProvider).selectedId;

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  Future<void> syncFromSession(SessionSummary? session) async {
    final gen = ++_syncGen;
    await _sub?.cancel();
    _sub = null;

    if (session == null) {
      state = const SessionRunState();
      return;
    }

    // Clear previous Session immediately so composer never shows Active for
    // the wrong thread while hydrate is in flight.
    final projection = session.activeRootRun;
    if (projection == null) {
      state = SessionRunState(
        boundSessionId: session.id,
        reconnectEpoch: state.reconnectEpoch,
      );
      return;
    }

    // Provisional Active from Session projection — hide Send until confirmed idle.
    state = SessionRunState(
      boundSessionId: session.id,
      activeRun: RunRecord(
        id: projection.id,
        sessionId: session.id,
        principalId: session.principalId,
        goal: projection.goal,
        status: projection.status,
        origin: projection.origin,
      ),
      reconnectEpoch: state.reconnectEpoch,
    );

    final client = _client;
    if (client == null) return;
    try {
      final run = await client.getRun(projection.id);
      if (gen != _syncGen || state.boundSessionId != session.id) return;
      state = SessionRunState(
        boundSessionId: session.id,
        activeRun: run,
        reconnectEpoch: state.reconnectEpoch,
      );
      if (run.isActive) {
        _subscribe(run.id, session.id, gen);
      }
    } catch (e) {
      if (gen != _syncGen || state.boundSessionId != session.id) return;
      state = state.copyWith(error: e.toString());
    }
  }

  /// After kill, background, or SSE loss: reload Run from Worker and resubscribe.
  ///
  /// Bumps [SessionRunState.reconnectEpoch] so the open Transcript reloads from
  /// the control plane (Worker remains system of record — no client write replica).
  Future<void> reconnect() async {
    if (_reconnectInFlight) return;
    _reconnectInFlight = true;
    try {
      await _reconnectBody();
    } finally {
      _reconnectInFlight = false;
    }
  }

  Future<void> _reconnectBody() async {
    final sessionId = state.boundSessionId ?? _selectedSessionId;
    if (sessionId == null) {
      await _ref.read(sessionsControllerProvider.notifier).refresh();
      return;
    }

    final gen = ++_syncGen;
    await _sub?.cancel();
    _sub = null;

    final client = _client;
    if (client == null) {
      state = state.copyWith(
        reconnectEpoch: state.reconnectEpoch + 1,
        error: 'Not connected',
      );
      return;
    }

    try {
      await _ref.read(sessionsControllerProvider.notifier).refresh();
      if (gen != _syncGen) return;

      final selected = _ref.read(sessionsControllerProvider).selected;
      final boundId = selected?.id ?? sessionId;

      String? runId = selected?.activeRootRun?.id ?? state.activeRun?.id;
      if (runId == null) {
        final detail = await client.getSession(boundId);
        if (gen != _syncGen) return;
        runId = detail.activeRootRun?.id;
        if (runId == null) {
          state = SessionRunState(
            boundSessionId: boundId,
            reconnectEpoch: state.reconnectEpoch + 1,
          );
          return;
        }
      }

      final run = await client.getRun(runId);
      if (gen != _syncGen) return;

      // Ephemeral stream state clears; durable tools reload via Transcript REST.
      state = SessionRunState(
        boundSessionId: boundId,
        activeRun: run,
        reconnectEpoch: state.reconnectEpoch + 1,
      );
      if (run.isActive) {
        _subscribe(run.id, boundId, gen);
      }
    } catch (e) {
      if (gen != _syncGen) return;
      state = state.copyWith(
        error: e.toString(),
        reconnectEpoch: state.reconnectEpoch + 1,
      );
    }
  }

  /// Idle composer: start root Run with message text as goal.
  /// Refuses if Active present locally or on Session projection.
  Future<void> startRun(
    String sessionId,
    String goal, {
    String? provider,
    String? model,
  }) async {
    final client = _client;
    if (client == null) {
      state = state.copyWith(error: 'Not connected');
      return;
    }
    if (state.boundSessionId != null && state.boundSessionId != sessionId) {
      state = state.copyWith(error: 'Session selection changed — try again');
      return;
    }
    if (state.hasActive && state.activeRun?.sessionId == sessionId) {
      state = state.copyWith(
        error:
            'Session already has an Active root Run — cancel or cancel-and-rerun',
      );
      return;
    }
    // Projection may show Active before controller hydrated.
    final selected = _ref.read(sessionsControllerProvider).selected;
    if (selected != null &&
        selected.id == sessionId &&
        selected.activeRootRun != null &&
        selected.activeRootRun!.status == 'active') {
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
      busy: true,
      clearError: true,
      streamingText: '',
      liveActivity: [],
    );
    try {
      final run = await client.startRun(
        sessionId,
        goal: goal.trim(),
        provider: provider,
        model: model,
      );
      if (state.boundSessionId != sessionId) return;
      state = state.copyWith(activeRun: run, busy: false);
      _subscribe(run.id, sessionId, _syncGen);
      await _ref.read(sessionsControllerProvider.notifier).refresh();
    } on KeryxApiException catch (e) {
      if (state.boundSessionId != sessionId) return;
      state = state.copyWith(busy: false, error: e.message);
    } catch (e) {
      if (state.boundSessionId != sessionId) return;
      state = state.copyWith(busy: false, error: e.toString());
    }
  }

  Future<void> cancel() async {
    final run = state.activeRun;
    final client = _client;
    final sessionId = state.boundSessionId ?? _selectedSessionId;
    if (run == null || client == null || sessionId == null) return;
    if (run.sessionId != sessionId) {
      state = state.copyWith(
        error: 'Cannot cancel a Run belonging to another Session',
      );
      return;
    }
    state = state.copyWith(busy: true, clearError: true);
    try {
      final cancelled = await client.cancelRun(run.id);
      await _sub?.cancel();
      _sub = null;
      if (state.boundSessionId != sessionId) return;
      // Parent cancel bounds the tree — project Child Runs stopped immediately.
      final live = markRunningChildrenStopped(
        state.liveActivity,
        cancelled.status == 'cancelled' ? 'cancelled' : cancelled.status,
      );
      state = state.copyWith(
        activeRun: cancelled,
        busy: false,
        liveActivity: live,
      );
      await _ref.read(sessionsControllerProvider.notifier).refresh();
    } on KeryxApiException catch (e) {
      if (state.boundSessionId != sessionId) return;
      state = state.copyWith(busy: false, error: e.message);
    }
  }

  /// Explicit cancel-and-rerun (no silent queue).
  Future<void> cancelAndRerun(
    String sessionId,
    String note, {
    String? provider,
    String? model,
  }) async {
    final run = state.activeRun;
    final client = _client;
    if (run == null || client == null) return;
    if (run.sessionId != sessionId || state.boundSessionId != sessionId) {
      state = state.copyWith(
        error: 'Cannot cancel-and-rerun a Run belonging to another Session',
      );
      return;
    }
    if (note.trim().isEmpty) {
      state = state.copyWith(error: 'Note required for cancel-and-rerun');
      return;
    }
    state = state.copyWith(
      busy: true,
      clearError: true,
      streamingText: '',
      liveActivity: [],
    );
    try {
      await _sub?.cancel();
      _sub = null;
      final next = await client.cancelAndRerun(
        sessionId,
        activeRunId: run.id,
        note: note.trim(),
        provider: provider,
        model: model,
      );
      if (state.boundSessionId != sessionId) return;
      state = state.copyWith(activeRun: next, busy: false);
      _subscribe(next.id, sessionId, _syncGen);
      await _ref.read(sessionsControllerProvider.notifier).refresh();
    } on KeryxApiException catch (e) {
      if (state.boundSessionId != sessionId) return;
      state = state.copyWith(busy: false, error: e.message);
    }
  }

  void _subscribe(String runId, String sessionId, int gen) {
    final client = _client;
    if (client == null) return;
    _sub?.cancel();
    _sub = client.streamRunEvents(runId).listen(
      (event) {
        if (gen != _syncGen || state.boundSessionId != sessionId) return;
        var live = state.liveActivity;
        final next = applyLiveActivity(live, event);
        if (next != null) live = next;
        var text = state.streamingText;
        final delta = event.deltaText;
        if (delta != null) text = '$text$delta';
        if (event.isTerminal) {
          unawaited(_refreshRun(runId, sessionId, gen));
        }
        state = state.copyWith(
          streamingText: text,
          liveActivity: live,
        );
      },
      onError: (Object e) {
        if (gen != _syncGen || state.boundSessionId != sessionId) return;
        // Network blip: reload Run truth + resubscribe when still Active.
        if (state.activeRun?.isActive == true) {
          unawaited(_reconnectAfterStreamLoss(gen));
        } else {
          state = state.copyWith(error: e.toString());
        }
      },
      onDone: () {
        if (gen != _syncGen || state.boundSessionId != sessionId) return;
        // Stream closed without terminal while Run still Active → resubscribe.
        if (state.activeRun?.isActive == true) {
          unawaited(_reconnectAfterStreamLoss(gen));
        }
      },
    );
  }

  Future<void> _reconnectAfterStreamLoss(int gen) async {
    if (gen != _syncGen) return;
    await Future<void>.delayed(const Duration(milliseconds: 350));
    if (gen != _syncGen) return;
    if (state.activeRun?.isActive != true) return;
    await reconnect();
  }

  Future<void> _refreshRun(String runId, String sessionId, int gen) async {
    final client = _client;
    if (client == null) return;
    try {
      final run = await client.getRun(runId);
      if (gen != _syncGen || state.boundSessionId != sessionId) return;
      state = state.copyWith(activeRun: run);
      await _ref.read(sessionsControllerProvider.notifier).refresh();
    } catch (_) {}
  }
}
