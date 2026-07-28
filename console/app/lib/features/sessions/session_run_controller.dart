import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../auth/auth_controller.dart';
import 'sessions_controller.dart';

/// Live Run + activity layer for the selected Session (tickets #41/#42).
final class SessionRunState {
  const SessionRunState({
    this.activeRun,
    this.streamingText = '',
    this.activity = const [],
    this.error,
    this.busy = false,
  });

  final RunRecord? activeRun;
  final String streamingText;
  final List<String> activity;
  final String? error;
  final bool busy;

  bool get hasActive => activeRun?.isActive == true;

  SessionRunState copyWith({
    RunRecord? activeRun,
    bool clearRun = false,
    String? streamingText,
    List<String>? activity,
    String? error,
    bool clearError = false,
    bool? busy,
  }) {
    return SessionRunState(
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

  KeryxApiClient? get _client =>
      _ref.read(authControllerProvider.notifier).client;

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  Future<void> syncFromSession(SessionSummary? session) async {
    await _sub?.cancel();
    _sub = null;
    if (session?.activeRootRun == null) {
      state = const SessionRunState();
      return;
    }
    final client = _client;
    if (client == null) return;
    try {
      final run = await client.getRun(session!.activeRootRun!.id);
      state = SessionRunState(activeRun: run);
      if (run.isActive) {
        _subscribe(run.id);
      }
    } catch (e) {
      state = state.copyWith(error: e.toString());
    }
  }

  /// Idle composer: start root Run with goal text. Refuses if Active present.
  Future<void> startRun(String sessionId, String goal, {String? provider, String? model}) async {
    final client = _client;
    if (client == null) {
      state = state.copyWith(error: 'Not connected');
      return;
    }
    if (state.hasActive) {
      state = state.copyWith(
        error: 'Session already has an Active root Run — cancel or cancel-and-rerun',
      );
      return;
    }
    if (goal.trim().isEmpty) {
      state = state.copyWith(error: 'Goal must not be empty');
      return;
    }
    state = state.copyWith(busy: true, clearError: true, streamingText: '', activity: []);
    try {
      final run = await client.startRun(
        sessionId,
        goal: goal.trim(),
        provider: provider,
        model: model,
      );
      state = state.copyWith(activeRun: run, busy: false);
      _subscribe(run.id);
      await _ref.read(sessionsControllerProvider.notifier).refresh();
    } on KeryxApiException catch (e) {
      state = state.copyWith(busy: false, error: e.message);
    } catch (e) {
      state = state.copyWith(busy: false, error: e.toString());
    }
  }

  Future<void> cancel() async {
    final run = state.activeRun;
    final client = _client;
    if (run == null || client == null) return;
    state = state.copyWith(busy: true, clearError: true);
    try {
      final cancelled = await client.cancelRun(run.id);
      await _sub?.cancel();
      _sub = null;
      state = state.copyWith(activeRun: cancelled, busy: false);
      await _ref.read(sessionsControllerProvider.notifier).refresh();
    } on KeryxApiException catch (e) {
      state = state.copyWith(busy: false, error: e.message);
    }
  }

  /// Explicit cancel-and-rerun (no silent queue).
  Future<void> cancelAndRerun(String sessionId, String note, {String? provider, String? model}) async {
    final run = state.activeRun;
    final client = _client;
    if (run == null || client == null) return;
    if (note.trim().isEmpty) {
      state = state.copyWith(error: 'Note required for cancel-and-rerun');
      return;
    }
    state = state.copyWith(busy: true, clearError: true, streamingText: '', activity: []);
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
      state = state.copyWith(activeRun: next, busy: false);
      _subscribe(next.id);
      await _ref.read(sessionsControllerProvider.notifier).refresh();
    } on KeryxApiException catch (e) {
      state = state.copyWith(busy: false, error: e.message);
    }
  }

  void _subscribe(String runId) {
    final client = _client;
    if (client == null) return;
    _sub?.cancel();
    _sub = client.streamRunEvents(runId).listen(
      (event) {
        var activity = [...state.activity, '${event.type}#${event.seq}'];
        if (activity.length > 40) {
          activity = activity.sublist(activity.length - 40);
        }
        var text = state.streamingText;
        final delta = event.deltaText;
        if (delta != null) text = '$text$delta';
        final tool = event.toolName;
        if (tool != null && tool.toLowerCase().contains('skill')) {
          if (!activity.any((a) => a.contains(tool))) {
            activity = [...activity, 'skill:$tool'];
          }
        }
        if (event.isTerminal) {
          unawaited(_refreshRun(runId));
        }
        state = state.copyWith(
          streamingText: text,
          activity: activity,
        );
      },
      onError: (Object e) {
        state = state.copyWith(error: e.toString());
      },
    );
  }

  Future<void> _refreshRun(String runId) async {
    final client = _client;
    if (client == null) return;
    try {
      final run = await client.getRun(runId);
      state = state.copyWith(activeRun: run);
      await _ref.read(sessionsControllerProvider.notifier).refresh();
    } catch (_) {}
  }
}
