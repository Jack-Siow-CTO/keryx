import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/console_chrome.dart';
import 'sessions_controller.dart';

/// Per-Session configuration: title rename; Policy/Workspace progressive disclosure.
///
/// Control plane may not expose Policy/Workspace edit yet — show honest empty state
/// rather than inventing a client-side Policy store (ADR 0031, ticket #62).
class SessionInfoPane extends ConsumerStatefulWidget {
  const SessionInfoPane({super.key, this.onClose});

  final VoidCallback? onClose;

  @override
  ConsumerState<SessionInfoPane> createState() => _SessionInfoPaneState();
}

class _SessionInfoPaneState extends ConsumerState<SessionInfoPane> {
  late final TextEditingController _title;
  bool _saving = false;

  @override
  void initState() {
    super.initState();
    final session = ref.read(sessionsControllerProvider).selected;
    _title = TextEditingController(text: session?.title ?? '');
  }

  @override
  void dispose() {
    _title.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final session = ref.watch(sessionsControllerProvider).selected;

    if (session == null) {
      return const ConsoleEmptyState(
        icon: Icons.info_outline,
        title: 'No Session selected',
        body: 'Open a chat to view Session info.',
      );
    }

    // Keep title field in sync when selection changes.
    ref.listen(sessionsControllerProvider, (prev, next) {
      if (prev?.selectedId != next.selectedId) {
        _title.text = next.selected?.title ?? '';
      }
    });

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 14, 8, 8),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  'Session info',
                  style: theme.textTheme.titleMedium,
                ),
              ),
              if (widget.onClose != null)
                IconButton(
                  tooltip: 'Close',
                  icon: const Icon(Icons.close, size: 20),
                  onPressed: widget.onClose,
                ),
            ],
          ),
        ),
        Divider(height: 1, color: theme.dividerTheme.color),
        Expanded(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
            children: [
              const ConsoleSectionLabel('Title'),
              TextField(
                controller: _title,
                decoration: const InputDecoration(
                  hintText: 'Session title',
                ),
                enabled: !_saving,
                onSubmitted: (_) => _saveTitle(session.id),
              ),
              const SizedBox(height: 10),
              Align(
                alignment: Alignment.centerRight,
                child: FilledButton.tonal(
                  onPressed: _saving ? null : () => _saveTitle(session.id),
                  child: Text(_saving ? 'Saving…' : 'Save title'),
                ),
              ),
              if (!session.titleIsCustom) ...[
                const SizedBox(height: 8),
                Text(
                  'Default title — rename for a human label, or leave it to derive from the first message.',
                  style: theme.textTheme.bodySmall,
                ),
              ],
              const SizedBox(height: 28),
              const ConsoleSectionLabel('Policy & Workspace'),
              const ConsoleBanner(
                tone: StatusPillTone.neutral,
                icon: Icons.lock_outline,
                message:
                    'Policy and Workspace roots are enforced on the Worker. '
                    'Edit is not exposed in Console yet — configure on the Worker. '
                    'No local Policy store.',
              ),
              const SizedBox(height: 20),
              const ConsoleSectionLabel('Identity'),
              Text(
                'Single agent identity (Worker). Child Runs are not separate contacts.',
                style: theme.textTheme.bodySmall,
              ),
              const SizedBox(height: 12),
              Text(
                'Session id: ${session.id}',
                style: theme.textTheme.labelSmall?.copyWith(
                  fontFamily: 'monospace',
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Future<void> _saveTitle(String sessionId) async {
    final name = _title.text.trim();
    if (name.isEmpty) return;
    setState(() => _saving = true);
    await ref.read(sessionsControllerProvider.notifier).rename(sessionId, name);
    if (mounted) setState(() => _saving = false);
  }
}
