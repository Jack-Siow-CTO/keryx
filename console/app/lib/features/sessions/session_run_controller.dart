import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../auth/auth_controller.dart';
import 'sessions_controller.dart';

/// Live Run + activity layer for the selected Session (tickets #41/#42, #59).
///
/// Always bound to [boundSessionId]. Stale getRun / SSE results for a previous
/// Session are ignored so Cancel/Send never target the wrong Session.
final class SessionRunState {
  const SessionRunState({
    this.boundSessionId,
    this.activeRun,
    this.streamingText = '',
    this.activity = const [],
    this.error,
    this.busy = false,
  });

  /// Session this controller state applies to (null when none selected).
  final String? boundSessionId;
  final RunRecord? activeRun;
  final String streamingText;

  /// Human-readable live activity lines (never raw `type#seq`).
  final List<String> activity;
  final String? error;
  final bool busy;

  bool get hasActive => activeRun?.isActive == true;

  SessionRunState copyWith({
    String? boundSessionId,
    bool clearBoundSession = false,
    RunRecord? activeRun,
    bool clearRun = false,
    String? streamingText,
    List<String>? activity,
    String? error,
    bool clearError = false,
    bool? busy,
  }) {
    return SessionRunState(
      boundSessionId:
          clearBoundSession ? null : (boundSessionId ?? this.boundSessionId),
      activeRun: clearRun ? null : (activeRun ?? this.activeRun),
      streamingText: streamingText ?? this.streamingText,
      activity: activity ?? this.activity,
      error: clearError ? null : (error ?? this.error),
      busy: busy ?? this.busy,
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
      state = SessionRunState(boundSessionId: session.id);
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
    );

    final client = _client;
    if (client == null) return;
    try {
      final run = await client.getRun(projection.id);
      if (gen != _syncGen || state.boundSessionId != session.id) return;
      state = SessionRunState(
        boundSessionId: session.id,
        activeRun: run,
      );
      if (run.isActive) {
        _subscribe(run.id, session.id, gen);
      }
    } catch (e) {
      if (gen != _syncGen || state.boundSessionId != session.id) return;
      state = state.copyWith(error: e.toString());
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
      activity: [],
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
      state = state.copyWith(activeRun: cancelled, busy: false);
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
      activity: [],
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
        var activity = [...state.activity];
        final human = _humanActivityLine(event);
        if (human != null) {
          activity = [...activity, human];
          if (activity.length > 40) {
            activity = activity.sublist(activity.length - 40);
          }
        }
        var text = state.streamingText;
        final delta = event.deltaText;
        if (delta != null) text = '$text$delta';
        if (event.isTerminal) {
          unawaited(_refreshRun(runId, sessionId, gen));
        }
        state = state.copyWith(
          streamingText: text,
          activity: activity,
        );
      },
      onError: (Object e) {
        if (gen != _syncGen || state.boundSessionId != sessionId) return;
        state = state.copyWith(error: e.toString());
      },
    );
  }

  /// Operator-readable activity — never raw `type#seq` in chrome.
  String? _humanActivityLine(RunEvent event) {
    final tool = event.toolName;
    if (tool != null && tool.isNotEmpty) {
      final phase = event.type.toLowerCase();
      if (phase.contains('start') || phase.contains('begin')) {
        return 'Tool · $tool started';
      }
      if (phase.contains('finish') ||
          phase.contains('end') ||
          phase.contains('complete') ||
          phase.contains('result')) {
        return 'Tool · $tool finished';
      }
      return 'Tool · $tool';
    }
    final t = event.type.toLowerCase();
    if (t.contains('delta') || t.contains('token') || t.contains('text')) {
      return null; // streaming text handles model deltas
    }
    if (t.contains('budget')) return 'Budget update';
    if (t.contains('error') || t.contains('fail')) return 'Run error';
    if (event.isTerminal) {
      if (t.contains('cancel')) return 'Run cancelled';
      if (t.contains('fail')) return 'Run failed';
      return 'Run completed';
    }
    return null;
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
