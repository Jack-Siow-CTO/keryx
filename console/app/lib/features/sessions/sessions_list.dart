import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../theme/keryx_theme.dart';
import '../../widgets/console_chrome.dart';
import '../inbox/inbox_screen.dart';
import 'sessions_controller.dart';

/// Chat list: Needs you system row + Session messenger rows (ADRs 0031, 0033).
class ChatListPane extends ConsumerStatefulWidget {
  const ChatListPane({
    super.key,
    required this.onSelectSession,
    required this.onSelectNeedsYou,
    required this.onNewChat,
    this.needsYouSelected = false,
  });

  final ValueChanged<String> onSelectSession;
  final VoidCallback onSelectNeedsYou;
  final ValueChanged<String> onNewChat;
  final bool needsYouSelected;

  @override
  ConsumerState<ChatListPane> createState() => _ChatListPaneState();
}

class _ChatListPaneState extends ConsumerState<ChatListPane> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(sessionsControllerProvider.notifier).refresh();
    });
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(sessionsControllerProvider);
    final inboxAsync = ref.watch(inboxProvider);
    final inboxCount = inboxAsync.maybeWhen(
      data: (items) => items.length,
      orElse: () => 0,
    );
    final theme = Theme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 10, 6, 4),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  'Chats',
                  style: theme.textTheme.titleSmall?.copyWith(
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ),
              IconButton(
                tooltip: 'Refresh',
                icon: const Icon(Icons.refresh, size: 18),
                visualDensity: VisualDensity.compact,
                onPressed: state.loading
                    ? null
                    : () {
                        ref.read(sessionsControllerProvider.notifier).refresh();
                        ref.invalidate(inboxProvider);
                      },
              ),
              IconButton(
                tooltip: 'New chat',
                icon: const Icon(Icons.edit_square, size: 20),
                visualDensity: VisualDensity.compact,
                onPressed: () => _newChat(),
              ),
            ],
          ),
        ),
        if (state.error != null)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: ConsoleBanner(message: state.error!),
          ),
        // Needs you system row — always at top of chat list (ADR 0033).
        Padding(
          padding: const EdgeInsets.fromLTRB(6, 4, 6, 2),
          child: _NeedsYouSystemRow(
            count: inboxCount,
            selected: widget.needsYouSelected,
            onTap: widget.onSelectNeedsYou,
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(14, 8, 14, 4),
          child: Text(
            'SESSIONS',
            style: theme.textTheme.labelSmall?.copyWith(
              fontWeight: FontWeight.w700,
              letterSpacing: 0.8,
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        Expanded(
          child: state.loading && state.sessions.isEmpty
              ? const ConsoleLoader()
              : state.sessions.isEmpty
                  ? ConsoleEmptyState(
                      icon: Icons.forum_outlined,
                      title: 'No chats yet',
                      body:
                          'Start a New chat to create an empty Session. Send starts the first Run. Needs you shows Approvals that need you.',
                      action: FilledButton.tonal(
                        onPressed: _newChat,
                        child: const Text('New chat'),
                      ),
                    )
                  : ListView.builder(
                      padding: const EdgeInsets.fromLTRB(6, 0, 6, 12),
                      itemCount: state.sessions.length,
                      itemBuilder: (context, i) {
                        final s = state.sessions[i];
                        final selected = s.id == state.selectedId &&
                            !widget.needsYouSelected;
                        return _ChatRow(
                          session: s,
                          selected: selected,
                          onTap: () async {
                            await ref
                                .read(sessionsControllerProvider.notifier)
                                .open(s.id);
                            widget.onSelectSession(s.id);
                          },
                        );
                      },
                    ),
        ),
      ],
    );
  }

  Future<void> _newChat() async {
    final created =
        await ref.read(sessionsControllerProvider.notifier).createSession();
    if (created != null) {
      widget.onNewChat(created.id);
    }
  }
}

class _NeedsYouSystemRow extends StatelessWidget {
  const _NeedsYouSystemRow({
    required this.count,
    required this.selected,
    required this.onTap,
  });

  final int count;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Material(
      color: selected
          ? theme.colorScheme.primary.withValues(alpha: 0.10)
          : theme.colorScheme.surfaceContainerLowest,
      borderRadius: BorderRadius.circular(10),
      child: InkWell(
        borderRadius: BorderRadius.circular(10),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(12, 11, 12, 11),
          child: Row(
            children: [
              Icon(
                Icons.priority_high_rounded,
                size: 18,
                color: count > 0
                    ? KeryxTheme.needsYou
                    : theme.colorScheme.onSurfaceVariant,
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'Needs you',
                      style: theme.textTheme.titleSmall?.copyWith(
                        fontWeight:
                            selected ? FontWeight.w700 : FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      count > 0
                          ? 'Approvals and failed Runs'
                          : 'All clear',
                      style: theme.textTheme.bodySmall,
                    ),
                  ],
                ),
              ),
              if (count > 0) AttentionBadge(count: count),
            ],
          ),
        ),
      ),
    );
  }
}

class _ChatRow extends StatelessWidget {
  const _ChatRow({
    required this.session,
    required this.selected,
    required this.onTap,
  });

  final SessionSummary session;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final active = session.activeRootRun != null;
    final pending = session.pendingApprovalCount;
    final timeLabel = _formatTime(session.updatedAt);

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Material(
        // Selection = filled panel tint, not left accent stripe (DESIGN.md).
        color: selected
            ? theme.colorScheme.primary.withValues(alpha: 0.10)
            : Colors.transparent,
        borderRadius: BorderRadius.circular(10),
        child: InkWell(
          borderRadius: BorderRadius.circular(10),
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 11, 10, 11),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        session.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.titleSmall?.copyWith(
                          fontWeight:
                              selected ? FontWeight.w700 : FontWeight.w600,
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Text(
                      timeLabel,
                      style: theme.textTheme.labelSmall?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                    if (pending > 0) ...[
                      const SizedBox(width: 8),
                      AttentionBadge(count: pending),
                    ] else if (active) ...[
                      const SizedBox(width: 8),
                      Container(
                        width: 8,
                        height: 8,
                        decoration: BoxDecoration(
                          color: theme.colorScheme.primary,
                          shape: BoxShape.circle,
                        ),
                      ),
                    ],
                  ],
                ),
                if (session.lastMessagePreview != null) ...[
                  const SizedBox(height: 3),
                  Text(
                    session.lastMessagePreview!,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall,
                  ),
                ],
                if (active) ...[
                  const SizedBox(height: 4),
                  Text(
                    'Active · ${session.activeRootRun!.goal}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.primary,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }

  String _formatTime(int epochSecs) {
    if (epochSecs <= 0) return '';
    final dt = DateTime.fromMillisecondsSinceEpoch(epochSecs * 1000);
    final now = DateTime.now();
    final sameDay =
        dt.year == now.year && dt.month == now.month && dt.day == now.day;
    if (sameDay) {
      final h = dt.hour.toString().padLeft(2, '0');
      final m = dt.minute.toString().padLeft(2, '0');
      return '$h:$m';
    }
    return '${dt.month}/${dt.day}';
  }
}

