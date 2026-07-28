import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../theme/keryx_theme.dart';
import '../../widgets/console_chrome.dart';
import '../auth/auth_controller.dart';
import '../inbox/inbox_screen.dart';
import 'sessions_controller.dart';

/// Sticky Approve/Deny card for the open Session (ADR 0033, ticket #61).
///
/// Dual surface with Needs you list row — not list-only, not full-app modal-only.
class StickyApprovalCard extends ConsumerWidget {
  const StickyApprovalCard({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final session = ref.watch(sessionsControllerProvider).selected;
    if (session == null) return const SizedBox.shrink();

    final inboxAsync = ref.watch(inboxProvider);
    final items = inboxAsync.maybeWhen(
      data: (list) => list,
      orElse: () => const <InboxItem>[],
    );

    final pending = items
        .where(
          (i) =>
              i.kind == 'approval_pending' &&
              i.sessionId == session.id &&
              i.approvalId != null,
        )
        .toList();

    // Fallback: Session projection says pending but Inbox not yet loaded.
    if (pending.isEmpty && session.pendingApprovalCount <= 0) {
      return const SizedBox.shrink();
    }
    if (pending.isEmpty) {
      return Material(
        elevation: 1,
        shadowColor: Theme.of(context).colorScheme.shadow.withValues(alpha: 0.08),
        color: KeryxTheme.needsYou.withValues(alpha: 0.08),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 12),
          child: Row(
            children: [
              const Icon(Icons.gavel_outlined, size: 18, color: KeryxTheme.needsYou),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  '${session.pendingApprovalCount} pending Approval${session.pendingApprovalCount == 1 ? '' : 's'} — open Needs you if actions are not listed yet.',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ),
            ],
          ),
        ),
      );
    }

    final item = pending.first;
    final theme = Theme.of(context);

    return Material(
      elevation: 2,
      shadowColor: theme.colorScheme.shadow.withValues(alpha: 0.10),
      color: theme.colorScheme.surface,
      child: DecoratedBox(
        decoration: BoxDecoration(
          border: Border(
            top: BorderSide(
              color: KeryxTheme.needsYou.withValues(alpha: 0.35),
            ),
          ),
          color: KeryxTheme.needsYou.withValues(alpha: 0.06),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  const Icon(
                    Icons.gavel_outlined,
                    size: 18,
                    color: KeryxTheme.needsYou,
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      item.title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.titleSmall?.copyWith(
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                  ),
                  const StatusPill(
                    label: 'Approval',
                    tone: StatusPillTone.attention,
                  ),
                ],
              ),
              if (item.summary.isNotEmpty) ...[
                const SizedBox(height: 6),
                Text(
                  item.summary,
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodySmall,
                ),
              ],
              const SizedBox(height: 12),
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  OutlinedButton(
                    onPressed: () => _decide(ref, item.approvalId!, false),
                    child: const Text('Deny'),
                  ),
                  const SizedBox(width: 8),
                  FilledButton(
                    style: FilledButton.styleFrom(
                      backgroundColor: KeryxTheme.needsYou,
                      foregroundColor: const Color(0xFFFFF8F4),
                    ),
                    onPressed: () => _decide(ref, item.approvalId!, true),
                    child: const Text('Approve'),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _decide(WidgetRef ref, String approvalId, bool approve) async {
    final client = ref.read(authControllerProvider.notifier).client;
    if (client == null) return;
    if (approve) {
      await client.approveApproval(approvalId);
    } else {
      await client.denyApproval(approvalId);
    }
    ref.invalidate(inboxProvider);
    await ref.read(sessionsControllerProvider.notifier).refresh();
  }
}
