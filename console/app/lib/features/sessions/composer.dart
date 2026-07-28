import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/run_preferences.dart';
import '../../widgets/console_chrome.dart';
import 'session_run_controller.dart';
import 'sessions_controller.dart';

/// Composer: idle **Send** starts a root Run; Active exposes cancel paths only.
///
/// ADRs 0016 + 0034. No client follow-up queue; never a silent second root Run.
/// Draft text is scoped per Session so switching chats never mis-sends.
class SessionComposer extends ConsumerStatefulWidget {
  const SessionComposer({super.key});

  @override
  ConsumerState<SessionComposer> createState() => _SessionComposerState();
}

class _SessionComposerState extends ConsumerState<SessionComposer> {
  final _text = TextEditingController();
  final _focus = FocusNode();
  final Map<String, String> _drafts = {};
  String? _draftSessionId;

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _persistCurrentDraft() {
    final id = _draftSessionId;
    if (id == null) return;
    final value = _text.text;
    if (value.isEmpty) {
      _drafts.remove(id);
    } else {
      _drafts[id] = value;
    }
  }

  void _loadDraftFor(String? sessionId) {
    _persistCurrentDraft();
    _draftSessionId = sessionId;
    final next = sessionId == null ? '' : (_drafts[sessionId] ?? '');
    if (_text.text != next) {
      _text.value = TextEditingValue(
        text: next,
        selection: TextSelection.collapsed(offset: next.length),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final session = ref.watch(sessionsControllerProvider).selected;
    final runState = ref.watch(sessionRunControllerProvider);
    final runPrefs = ref.watch(runPreferencesProvider);
    final theme = Theme.of(context);

    if (session == null) return const SizedBox.shrink();

    // Keep draft buffer bound to selected Session.
    if (_draftSessionId != session.id) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _loadDraftFor(session.id);
      });
    }

    // Active if controller says so for this Session, or projection still Active.
    final projectionActive = session.activeRootRun?.status == 'active';
    final controllerActive = runState.hasActive &&
        (runState.boundSessionId == null ||
            runState.boundSessionId == session.id) &&
        (runState.activeRun == null ||
            runState.activeRun!.sessionId == session.id);
    final active = controllerActive || projectionActive;

    ref.listen(sessionsControllerProvider, (prev, next) {
      if (prev?.selectedId != next.selectedId) {
        _loadDraftFor(next.selectedId);
        ref
            .read(sessionRunControllerProvider.notifier)
            .syncFromSession(next.selected);
        if (next.selected != null) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted) _focus.requestFocus();
          });
        }
      }
    });

    final providerLabel = runPrefs.selectedProvider?.displayName ??
        runPrefs.provider ??
        'Worker default';
    final modelLabel = runPrefs.model;

    return Material(
      elevation: 2,
      shadowColor: theme.colorScheme.shadow.withValues(alpha: 0.08),
      color: theme.colorScheme.surface,
      child: DecoratedBox(
        decoration: BoxDecoration(
          border: Border(
            top: BorderSide(
              color: theme.dividerTheme.color ?? theme.dividerColor,
            ),
          ),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 14),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              if (runState.error != null &&
                  (runState.boundSessionId == null ||
                      runState.boundSessionId == session.id)) ...[
                ConsoleBanner(message: runState.error!),
                const SizedBox(height: 10),
              ],
              if (active) ...[
                const StatusPill(
                  icon: Icons.pause_circle_outline,
                  label:
                      'Active Run — wait, Cancel, or Cancel & re-run. Send disabled.',
                  tone: StatusPillTone.active,
                ),
                const SizedBox(height: 10),
              ] else if (modelLabel != null || runPrefs.provider != null) ...[
                Align(
                  alignment: Alignment.centerLeft,
                  child: Text(
                    modelLabel != null
                        ? '$providerLabel · $modelLabel'
                        : providerLabel,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ),
                const SizedBox(height: 8),
              ],
              // Message field + primary actions share one row (messenger dock).
              Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Expanded(
                    child: TextField(
                      controller: _text,
                      focusNode: _focus,
                      minLines: 1,
                      maxLines: 4,
                      textInputAction: TextInputAction.send,
                      decoration: InputDecoration(
                        hintText: active
                            ? 'Optional note for cancel-and-re-run…'
                            : 'Message the agent…',
                        isDense: true,
                        contentPadding: const EdgeInsets.symmetric(
                          horizontal: 14,
                          vertical: 12,
                        ),
                      ),
                      enabled: !runState.busy,
                      onChanged: (_) => _persistCurrentDraft(),
                      onSubmitted: (_) {
                        if (!active) _submit(session.id, active: false);
                      },
                    ),
                  ),
                  const SizedBox(width: 10),
                  if (!active)
                    FilledButton.icon(
                      onPressed: runState.busy
                          ? null
                          : () => _submit(session.id, active: false),
                      icon: const Icon(Icons.send, size: 16),
                      label: const Text('Send'),
                    )
                  else
                    Row(
                      mainAxisSize: MainAxisSize.min,
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
                              : () => _submit(session.id, active: true),
                          child: const Text('Cancel & re-run'),
                        ),
                      ],
                    ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _submit(String sessionId, {required bool active}) async {
    final text = _text.text;
    final prefs = ref.read(runPreferencesProvider);
    final notifier = ref.read(sessionRunControllerProvider.notifier);
    if (active) {
      await notifier.cancelAndRerun(
        sessionId,
        text,
        provider: prefs.provider,
        model: prefs.model,
      );
    } else {
      await notifier.startRun(
        sessionId,
        text,
        provider: prefs.provider,
        model: prefs.model,
      );
    }
    if (mounted && ref.read(sessionRunControllerProvider).error == null) {
      _text.clear();
      _drafts.remove(sessionId);
    }
  }
}
