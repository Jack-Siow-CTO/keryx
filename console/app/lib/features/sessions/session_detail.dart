import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'composer.dart';
import 'session_run_controller.dart';
import 'sessions_controller.dart';
import 'transcript_pane.dart';

/// Session main pane: header chips, Transcript, live activity, composer.
class SessionDetailPane extends ConsumerStatefulWidget {
  const SessionDetailPane({super.key});

  @override
  ConsumerState<SessionDetailPane> createState() => _SessionDetailPaneState();
}

class _SessionDetailPaneState extends ConsumerState<SessionDetailPane> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final s = ref.read(sessionsControllerProvider).selected;
      ref.read(sessionRunControllerProvider.notifier).syncFromSession(s);
    });
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(sessionsControllerProvider);
    final runState = ref.watch(sessionRunControllerProvider);
    final session = state.selected;
    final theme = Theme.of(context);

    if (session == null) {
      return Center(
        child: Text(
          'Select a Session',
          style: theme.textTheme.titleMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Material(
          color: theme.colorScheme.surfaceContainerLow,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  session.title,
                  style: theme.textTheme.titleLarge?.copyWith(
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: 8),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    if (session.activeRootRun != null ||
                        runState.activeRun?.isActive == true)
                      Chip(
                        avatar: const Icon(Icons.play_circle_outline, size: 18),
                        label: Text(
                          'Active · ${runState.activeRun?.origin ?? session.activeRootRun?.origin ?? "control_plane"}',
                        ),
                        visualDensity: VisualDensity.compact,
                      ),
                    if (session.pendingApprovalCount > 0)
                      Chip(
                        avatar: const Icon(Icons.gavel_outlined, size: 18),
                        label: Text(
                          '${session.pendingApprovalCount} pending Approval(s)',
                        ),
                        visualDensity: VisualDensity.compact,
                      ),
                    if (!session.titleIsCustom)
                      Chip(
                        label: const Text('Default title'),
                        visualDensity: VisualDensity.compact,
                        side: BorderSide(
                          color: theme.colorScheme.outlineVariant,
                        ),
                      ),
                  ],
                ),
                if (session.activeRootRun != null || runState.activeRun != null) ...[
                  const SizedBox(height: 4),
                  Text(
                    runState.activeRun?.goal ?? session.activeRootRun!.goal,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
        const Divider(height: 1),
        const Expanded(child: SessionConversationBody()),
        if (runState.streamingText.isNotEmpty)
          Material(
            color: theme.colorScheme.surfaceContainerHighest,
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Text(
                runState.streamingText,
                style: theme.textTheme.bodyMedium,
              ),
            ),
          ),
        const SessionComposer(),
      ],
    );
  }
}
