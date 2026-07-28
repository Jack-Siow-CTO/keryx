import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../auth/auth_controller.dart';
import 'session_run_controller.dart';
import 'sessions_controller.dart';

/// Composer modes (ADR 0016): idle → start Run; Active → cancel / cancel-and-rerun.
class SessionComposer extends ConsumerStatefulWidget {
  const SessionComposer({super.key});

  @override
  ConsumerState<SessionComposer> createState() => _SessionComposerState();
}

class _SessionComposerState extends ConsumerState<SessionComposer> {
  final _text = TextEditingController();
  String? _provider;
  String? _model;
  List<ProviderInfo> _providers = const [];

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadProviders());
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  Future<void> _loadProviders() async {
    final client = ref.read(authControllerProvider.notifier).client;
    if (client == null) return;
    try {
      final res = await client.listProviders();
      if (!mounted) return;
      setState(() {
        _providers = res.providers.where((p) => p.registered).toList();
        _provider = res.defaultProvider ??
            (_providers.isNotEmpty ? _providers.first.name : null);
        if (_provider != null) {
          final p = _providers.where((x) => x.name == _provider).firstOrNull;
          _model = p?.defaultModel.isNotEmpty == true
              ? p!.defaultModel
              : (p?.models.isNotEmpty == true ? p!.models.first : null);
        }
      });
    } catch (_) {}
  }

  @override
  Widget build(BuildContext context) {
    final session = ref.watch(sessionsControllerProvider).selected;
    final runState = ref.watch(sessionRunControllerProvider);
    final theme = Theme.of(context);

    if (session == null) return const SizedBox.shrink();

    final active = runState.hasActive;

    // Keep run controller in sync with selection.
    ref.listen(sessionsControllerProvider, (prev, next) {
      if (prev?.selectedId != next.selectedId) {
        ref.read(sessionRunControllerProvider.notifier).syncFromSession(next.selected);
      }
    });

    return Material(
      elevation: 2,
      color: theme.colorScheme.surface,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            if (runState.error != null)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Text(
                  runState.error!,
                  style: TextStyle(color: theme.colorScheme.error, fontSize: 12),
                ),
              ),
            if (active) ...[
              Text(
                'Active Run — send is disabled. Choose cancel or cancel-and-rerun.',
                style: theme.textTheme.labelMedium?.copyWith(
                  color: theme.colorScheme.primary,
                ),
              ),
              const SizedBox(height: 8),
            ],
            if (_providers.isNotEmpty && !active)
              Row(
                children: [
                  Expanded(
                    child: DropdownButtonFormField<String>(
                      initialValue: _provider,
                      decoration: const InputDecoration(
                        labelText: 'Provider',
                        isDense: true,
                      ),
                      items: _providers
                          .map(
                            (p) => DropdownMenuItem(
                              value: p.name,
                              child: Text(p.displayName),
                            ),
                          )
                          .toList(),
                      onChanged: (v) {
                        setState(() {
                          _provider = v;
                          final p =
                              _providers.where((x) => x.name == v).firstOrNull;
                          _model = p?.defaultModel.isNotEmpty == true
                              ? p!.defaultModel
                              : (p?.models.isNotEmpty == true
                                  ? p!.models.first
                                  : null);
                        });
                      },
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: DropdownButtonFormField<String>(
                      initialValue: _model,
                      decoration: const InputDecoration(
                        labelText: 'Model',
                        isDense: true,
                      ),
                      items: () {
                        final p = _providers
                            .where((x) => x.name == _provider)
                            .firstOrNull;
                        final models = p?.models ?? const <String>[];
                        final def = p?.defaultModel;
                        final all = {
                          if (def != null && def.isNotEmpty) def,
                          ...models,
                        }.toList();
                        return all
                            .map(
                              (m) => DropdownMenuItem(value: m, child: Text(m)),
                            )
                            .toList();
                      }(),
                      onChanged: (v) => setState(() => _model = v),
                    ),
                  ),
                ],
              ),
            if (_providers.isNotEmpty && !active) const SizedBox(height: 8),
            TextField(
              controller: _text,
              minLines: 1,
              maxLines: 4,
              decoration: InputDecoration(
                hintText: active
                    ? 'Note for cancel-and-rerun…'
                    : 'Start a Run with this goal…',
                border: const OutlineInputBorder(),
              ),
              enabled: !runState.busy,
              onSubmitted: (_) => _submit(session.id, active),
            ),
            const SizedBox(height: 8),
            if (!active)
              Align(
                alignment: Alignment.centerRight,
                child: FilledButton.icon(
                  onPressed: runState.busy
                      ? null
                      : () => _submit(session.id, false),
                  icon: const Icon(Icons.send, size: 18),
                  label: const Text('Start Run'),
                ),
              )
            else
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  OutlinedButton(
                    onPressed: runState.busy
                        ? null
                        : () => ref
                            .read(sessionRunControllerProvider.notifier)
                            .cancel(),
                    child: const Text('Cancel Run'),
                  ),
                  const SizedBox(width: 8),
                  FilledButton(
                    onPressed: runState.busy
                        ? null
                        : () => _submit(session.id, true),
                    child: const Text('Cancel & re-run'),
                  ),
                ],
              ),
            if (runState.streamingText.isNotEmpty) ...[
              const SizedBox(height: 8),
              Text('Live', style: theme.textTheme.labelSmall),
              Text(runState.streamingText, style: theme.textTheme.bodySmall),
            ],
            if (runState.activity.isNotEmpty) ...[
              const SizedBox(height: 4),
              Text(
                'Activity: ${runState.activity.take(5).join(" · ")}',
                style: theme.textTheme.labelSmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
              ),
            ],
          ],
        ),
      ),
    );
  }

  Future<void> _submit(String sessionId, bool active) async {
    final text = _text.text;
    final notifier = ref.read(sessionRunControllerProvider.notifier);
    if (active) {
      await notifier.cancelAndRerun(
        sessionId,
        text,
        provider: _provider,
        model: _model,
      );
    } else {
      await notifier.startRun(
        sessionId,
        text,
        provider: _provider,
        model: _model,
      );
    }
    if (mounted && ref.read(sessionRunControllerProvider).error == null) {
      _text.clear();
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
