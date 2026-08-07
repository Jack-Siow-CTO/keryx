import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/console_chrome.dart';
import 'composer.dart';
import 'session_run_controller.dart';
import 'sessions_controller.dart';
import 'sticky_approval.dart';
import 'transcript_pane.dart';

/// Session thread: header (agent + title), layered Transcript, sticky Approval, composer.
class SessionDetailPane extends ConsumerStatefulWidget {
  const SessionDetailPane({
    super.key,
    this.showHeader = true,
    this.onOpenSessionInfo,
    this.onOpenArtifact,
  });

  final bool showHeader;
  final VoidCallback? onOpenSessionInfo;
  final ValueChanged<String>? onOpenArtifact;

  @override
  ConsumerState<SessionDetailPane> createState() => _SessionDetailPaneState();
}

class _SessionDetailPaneState extends ConsumerState<SessionDetailPane>
    with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final s = ref.read(sessionsControllerProvider).selected;
      ref.read(sessionRunControllerProvider.notifier).syncFromSession(s);
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  /// After background / kill / network blip: reload Session truth + SSE resubscribe.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      unawaited(ref.read(sessionRunControllerProvider.notifier).reconnect());
    }
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(sessionsControllerProvider);
    final runState = ref.watch(sessionRunControllerProvider);
    final session = state.selected;
    final theme = Theme.of(context);

    if (session == null) {
      return const ConsoleEmptyState(
        icon: Icons.chat_bubble_outline,
        title: 'Select a chat',
        body:
            'Open a Session from the chat list, or start a New chat. Send starts a root Run when idle.',
      );
    }

    final active = session.activeRootRun?.status == 'active' ||
        (runState.hasActive &&
            (runState.boundSessionId == null ||
                runState.boundSessionId == session.id));
    final pending = session.pendingApprovalCount;

    return ColoredBox(
      color: theme.colorScheme.surfaceContainerLowest,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (widget.showHeader)
            Material(
              color: theme.colorScheme.surface,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 12, 8, 12),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Keryx',
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                              fontWeight: FontWeight.w600,
                              letterSpacing: 0.4,
                            ),
                          ),
                          const SizedBox(height: 2),
                          Text(
                            session.title,
                            style: theme.textTheme.titleMedium,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                          ),
                          const SizedBox(height: 8),
                          Wrap(
                            spacing: 8,
                            runSpacing: 6,
                            children: [
                              if (active)
                                const StatusPill(
                                  icon: Icons.play_circle_outline,
                                  label: 'Active Run',
                                  tone: StatusPillTone.active,
                                ),
                              if (pending > 0)
                                StatusPill(
                                  icon: Icons.gavel_outlined,
                                  label:
                                      '$pending pending Approval${pending == 1 ? '' : 's'}',
                                  tone: StatusPillTone.attention,
                                ),
                            ],
                          ),
                        ],
                      ),
                    ),
                    if (widget.onOpenSessionInfo != null)
                      IconButton(
                        tooltip: 'Session info',
                        icon: const Icon(Icons.info_outline, size: 20),
                        onPressed: widget.onOpenSessionInfo,
                      ),
                  ],
                ),
              ),
            ),
          if (widget.showHeader)
            Divider(height: 1, color: theme.dividerTheme.color),
          if (active)
            _StreamingStatusStrip(
              goal: runState.activeRun?.goal ??
                  session.activeRootRun?.goal ??
                  '',
              // Prefer human activity lines; never fall back to raw event noise.
              activitySnippet: runState.lastActivitySnippet,
            ),
          // Child Runs under parent root (read-only tree; not Session contacts).
          if (runState.boundSessionId == null ||
              runState.boundSessionId == session.id)
            if (runState.childRunTree case final tree?)
              ChildRunTreeStrip(tree: tree),
          Expanded(
            child: SessionConversationBody(
              onOpenArtifact: widget.onOpenArtifact,
            ),
          ),
          if (runState.streamingText.isNotEmpty)
            Material(
              color: theme.colorScheme.primary.withValues(alpha: 0.06),
              child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 10, 16, 12),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'Agent',
                      style: theme.textTheme.labelSmall?.copyWith(
                        fontWeight: FontWeight.w700,
                        color: theme.colorScheme.primary,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      runState.streamingText,
                      style: theme.textTheme.bodyMedium?.copyWith(height: 1.45),
                    ),
                  ],
                ),
              ),
            ),
          const StickyApprovalCard(),
          const SessionComposer(),
        ],
      ),
    );
  }
}

class _StreamingStatusStrip extends StatelessWidget {
  const _StreamingStatusStrip({
    required this.goal,
    this.activitySnippet,
  });

  final String goal;
  final String? activitySnippet;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Material(
      color: theme.colorScheme.primary.withValues(alpha: 0.05),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
        child: Row(
          children: [
            SizedBox(
              width: 10,
              height: 10,
              child: CircularProgressIndicator(
                strokeWidth: 1.5,
                color: theme.colorScheme.primary,
              ),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Text(
                activitySnippet != null && activitySnippet!.isNotEmpty
                    ? activitySnippet!
                    : (goal.isEmpty ? 'Run in progress…' : goal),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: theme.textTheme.labelMedium?.copyWith(
                  color: theme.colorScheme.primary,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Read-only Child Run tree under the Active root (#83).
///
/// Projects control-plane linkage in the open Session. Child Runs are never
/// listed as separate chat contacts.
class ChildRunTreeStrip extends StatelessWidget {
  const ChildRunTreeStrip({super.key, required this.tree});

  final ChildRunTree tree;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final rootStopped = !tree.rootIsActive;
    final border = theme.dividerTheme.color ?? theme.dividerColor;

    return Material(
      color: theme.colorScheme.surfaceContainerLow,
      child: DecoratedBox(
        decoration: BoxDecoration(
          border: Border(
            bottom: BorderSide(color: border),
          ),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(16, 10, 16, 12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Icon(
                    Icons.account_tree_outlined,
                    size: 16,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Root Run · ${tree.rootStatus}',
                      style: theme.textTheme.labelMedium?.copyWith(
                        fontWeight: FontWeight.w700,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  if (rootStopped)
                    Text(
                      tree.rootStatus == 'cancelled'
                          ? 'parent stop'
                          : tree.rootStatus,
                      style: theme.textTheme.labelSmall?.copyWith(
                        color: theme.colorScheme.error,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                ],
              ),
              if (tree.rootGoal.isNotEmpty) ...[
                const SizedBox(height: 2),
                Padding(
                  padding: const EdgeInsets.only(left: 24),
                  child: Text(
                    tree.rootGoal,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ),
              ],
              const SizedBox(height: 8),
              for (final child in tree.children) ...[
                Padding(
                  padding: const EdgeInsets.only(left: 12, top: 4),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '└ ',
                        style: theme.textTheme.labelMedium?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                          fontFamily: 'monospace',
                        ),
                      ),
                      Icon(
                        Icons.subdirectory_arrow_right,
                        size: 14,
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                      const SizedBox(width: 4),
                      Expanded(
                        child: Text(
                          child.goal.isEmpty
                              ? 'Child Run'
                              : child.goal,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodySmall,
                        ),
                      ),
                      const SizedBox(width: 8),
                      Text(
                        child.status,
                        style: theme.textTheme.labelSmall?.copyWith(
                          color: child.isRunning
                              ? theme.colorScheme.primary
                              : (child.status == 'cancelled' ||
                                      child.status == 'failed'
                                  ? theme.colorScheme.error
                                  : theme.colorScheme.onSurfaceVariant),
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
              if (rootStopped && tree.children.any((c) => c.status == 'cancelled'))
                Padding(
                  padding: const EdgeInsets.only(left: 24, top: 8),
                  child: Text(
                    'Cancel stops the root and Child Runs together.',
                    style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
