import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../auth/auth_controller.dart';

final class SessionsState {
  const SessionsState({
    this.sessions = const [],
    this.selectedId,
    this.loading = false,
    this.error,
  });

  final List<SessionSummary> sessions;
  final String? selectedId;
  final bool loading;
  final String? error;

  SessionSummary? get selected {
    final id = selectedId;
    if (id == null) return null;
    for (final s in sessions) {
      if (s.id == id) return s;
    }
    return null;
  }

  SessionsState copyWith({
    List<SessionSummary>? sessions,
    String? selectedId,
    bool clearSelected = false,
    bool? loading,
    String? error,
    bool clearError = false,
  }) {
    return SessionsState(
      sessions: sessions ?? this.sessions,
      selectedId: clearSelected ? null : (selectedId ?? this.selectedId),
      loading: loading ?? this.loading,
      error: clearError ? null : (error ?? this.error),
    );
  }
}

final sessionsControllerProvider =
    StateNotifierProvider<SessionsController, SessionsState>((ref) {
  return SessionsController(ref);
});

class SessionsController extends StateNotifier<SessionsState> {
  SessionsController(this._ref) : super(const SessionsState());

  final Ref _ref;

  KeryxApiClient? get _client =>
      _ref.read(authControllerProvider.notifier).client;

  Future<void> refresh() async {
    final client = _client;
    if (client == null) {
      state = state.copyWith(error: 'Not connected');
      return;
    }
    state = state.copyWith(loading: true, clearError: true);
    try {
      final list = await client.listSessions();
      state = state.copyWith(
        sessions: list.sessions,
        loading: false,
      );
    } on KeryxApiException catch (e) {
      state = state.copyWith(loading: false, error: e.message);
    } catch (e) {
      state = state.copyWith(loading: false, error: e.toString());
    }
  }

  Future<SessionSummary?> createSession() async {
    final client = _client;
    if (client == null) return null;
    try {
      final created = await client.createSession();
      await refresh();
      state = state.copyWith(selectedId: created.id);
      return created;
    } on KeryxApiException catch (e) {
      state = state.copyWith(error: e.message);
      return null;
    }
  }

  Future<void> rename(String sessionId, String title) async {
    final client = _client;
    if (client == null) return;
    try {
      await client.patchSessionTitle(sessionId, title: title);
      await refresh();
    } on KeryxApiException catch (e) {
      state = state.copyWith(error: e.message);
    }
  }

  void select(String? id) {
    state = state.copyWith(selectedId: id, clearSelected: id == null);
  }

  Future<void> open(String sessionId) async {
    final client = _client;
    if (client == null) return;
    try {
      final detail = await client.getSession(sessionId);
      final updated = [
        for (final s in state.sessions)
          if (s.id == sessionId) detail else s,
      ];
      if (!updated.any((s) => s.id == sessionId)) {
        updated.insert(0, detail);
      }
      state = state.copyWith(sessions: updated, selectedId: sessionId);
    } on KeryxApiException catch (e) {
      state = state.copyWith(error: e.message);
    }
  }
}
