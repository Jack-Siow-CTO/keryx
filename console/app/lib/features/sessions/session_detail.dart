import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'sessions_controller.dart';
import 'transcript_pane.dart';

/// Session main pane shell: header chips for Active Run; Transcript later (#40).
class SessionDetailPane extends ConsumerWidget {
  const SessionDetailPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(sessionsControllerProvider);
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
                    if (session.activeRootRun != null)
                      Chip(
                        avatar: const Icon(Icons.play_circle_outline, size: 18),
                        label: Text(
                          'Active · ${session.activeRootRun!.origin}',
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
                        side: BorderSide(color: theme.colorScheme.outlineVariant),
                      ),
                  ],
                ),
                if (session.activeRootRun != null) ...[
                  const SizedBox(height: 4),
                  Text(
                    session.activeRootRun!.goal,
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
      ],
    );
  }
}
