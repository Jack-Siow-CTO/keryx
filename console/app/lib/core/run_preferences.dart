import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../features/auth/auth_controller.dart';

const _kProvider = 'keryx.run.provider';
const _kModel = 'keryx.run.model';

/// Default LLM provider/model for Send / root Run (console-local prefs, not secrets).
final class RunPreferences {
  const RunPreferences({
    this.provider,
    this.model,
    this.providers = const [],
    this.loading = false,
    this.error,
  });

  final String? provider;
  final String? model;
  final List<ProviderInfo> providers;
  final bool loading;
  final String? error;

  ProviderInfo? get selectedProvider {
    final name = provider;
    if (name == null) return null;
    for (final p in providers) {
      if (p.name == name) return p;
    }
    return null;
  }

  List<String> get modelsForSelected {
    final p = selectedProvider;
    if (p == null) return const [];
    final all = <String>{
      if (p.defaultModel.isNotEmpty) p.defaultModel,
      ...p.models,
    };
    return all.toList();
  }

  RunPreferences copyWith({
    String? provider,
    String? model,
    List<ProviderInfo>? providers,
    bool? loading,
    String? error,
    bool clearError = false,
    bool clearProvider = false,
    bool clearModel = false,
  }) {
    return RunPreferences(
      provider: clearProvider ? null : (provider ?? this.provider),
      model: clearModel ? null : (model ?? this.model),
      providers: providers ?? this.providers,
      loading: loading ?? this.loading,
      error: clearError ? null : (error ?? this.error),
    );
  }
}

final runPreferencesProvider =
    StateNotifierProvider<RunPreferencesController, RunPreferences>((ref) {
  final controller = RunPreferencesController(ref);
  // Refresh when auth becomes ready / client available.
  ref.listen(authControllerProvider, (prev, next) {
    if (next.status == AuthStatus.ready) {
      controller.refresh();
    }
  });
  return controller;
});

class RunPreferencesController extends StateNotifier<RunPreferences> {
  RunPreferencesController(this._ref) : super(const RunPreferences()) {
    _bootstrap();
  }

  final Ref _ref;

  Future<void> _bootstrap() async {
    final prefs = await SharedPreferences.getInstance();
    final savedProvider = prefs.getString(_kProvider);
    final savedModel = prefs.getString(_kModel);
    state = state.copyWith(
      provider: savedProvider,
      model: savedModel,
    );
    await refresh();
  }

  Future<void> refresh() async {
    final client = _ref.read(authControllerProvider.notifier).client;
    if (client == null) {
      state = state.copyWith(
        providers: const [],
        loading: false,
        clearError: true,
      );
      return;
    }

    state = state.copyWith(loading: true, clearError: true);
    try {
      final res = await client.listProviders();
      final registered =
          res.providers.where((p) => p.registered).toList(growable: false);

      var provider = state.provider;
      if (provider == null ||
          !registered.any((p) => p.name == provider)) {
        provider = res.defaultProvider ??
            (registered.isNotEmpty ? registered.first.name : null);
      }

      String? model = state.model;
      final selected = registered.where((p) => p.name == provider).firstOrNull;
      if (selected != null) {
        final models = <String>{
          if (selected.defaultModel.isNotEmpty) selected.defaultModel,
          ...selected.models,
        };
        if (model == null || !models.contains(model)) {
          model = selected.defaultModel.isNotEmpty
              ? selected.defaultModel
              : (selected.models.isNotEmpty ? selected.models.first : null);
        }
      } else {
        model = null;
      }

      state = RunPreferences(
        provider: provider,
        model: model,
        providers: registered,
        loading: false,
      );
      await _persist(provider: provider, model: model);
    } catch (e) {
      state = state.copyWith(
        loading: false,
        error: e.toString(),
      );
    }
  }

  Future<void> setProvider(String? name) async {
    final p = state.providers.where((x) => x.name == name).firstOrNull;
    final model = p == null
        ? null
        : (p.defaultModel.isNotEmpty
            ? p.defaultModel
            : (p.models.isNotEmpty ? p.models.first : null));
    state = state.copyWith(
      provider: name,
      model: model,
      clearProvider: name == null,
      clearModel: model == null,
    );
    await _persist(provider: name, model: model);
  }

  Future<void> setModel(String? model) async {
    state = state.copyWith(
      model: model,
      clearModel: model == null,
    );
    await _persist(provider: state.provider, model: model);
  }

  Future<void> _persist({String? provider, String? model}) async {
    final prefs = await SharedPreferences.getInstance();
    if (provider == null || provider.isEmpty) {
      await prefs.remove(_kProvider);
    } else {
      await prefs.setString(_kProvider, provider);
    }
    if (model == null || model.isEmpty) {
      await prefs.remove(_kModel);
    } else {
      await prefs.setString(_kModel, model);
    }
  }
}

extension<T> on Iterable<T> {
  T? get firstOrNull {
    final it = iterator;
    if (it.moveNext()) return it.current;
    return null;
  }
}
