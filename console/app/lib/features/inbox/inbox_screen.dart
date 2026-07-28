import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../theme/keryx_theme.dart';
import '../../widgets/console_chrome.dart';
import '../auth/auth_controller.dart';
import '../sessions/sessions_controller.dart';

final inboxProvider = FutureProvider.autoDispose<List<InboxItem>>((ref) async {
  final client = ref.watch(authControllerProvider.notifier).client;
  if (client == null) return const [];
  return client.listInbox();
});

/// Needs you content: Inbox read projection (Approvals, failed/interrupted Runs).
///
/// Opened from the chat-list system row — not a permanent dual-rail peer (ADR 0033).
class NeedsYouPane extends ConsumerWidget {
  const NeedsYouPane({
    super.key,
    this.onOpenSession,
  });

  /// Called after selecting an item that deep-links to a Session.
  final ValueChanged<String>? onOpenSession;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final async = ref.watch(inboxProvider);
    final theme = Theme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 14, 8, 8),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  'Needs you',
                  style: theme.textTheme.titleMedium,
                ),
              ),
              IconButton(
                tooltip: 'Refresh',
                icon: const Icon(Icons.refresh, size: 18),
                visualDensity: VisualDensity.compact,
                onPressed: () => ref.invalidate(inboxProvider),
              ),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
          child: Text(
            'Cross-Session Approvals and failed Runs. Resolving an Approval clears attention — no mark-as-read.',
            style: theme.textTheme.bodySmall,
          ),
        ),
        Divider(height: 1, color: theme.dividerTheme.color),
        Expanded(
          child: async.when(
            loading: () => const ConsoleLoader(),
            error: (e, _) => Padding(
              padding: const EdgeInsets.all(12),
              child: ConsoleBanner(message: '$e'),
            ),
            data: (items) {
              if (items.isEmpty) {
                return const ConsoleEmptyState(
                  icon: Icons.check_circle_outline,
                  title: 'All clear',
                  body:
                      'Pending Approvals and failed Runs appear here when they need you.',
                );
              }
              return ListView.separated(
                padding: const EdgeInsets.fromLTRB(8, 8, 8, 12),
                itemCount: items.length,
                separatorBuilder: (_, __) => const SizedBox(height: 6),
                itemBuilder: (context, i) {
                  final item = items[i];
                  final isApproval = item.kind == 'approval_pending';
                  return Material(
                    color: isApproval
                        ? KeryxTheme.needsYou.withValues(alpha: 0.06)
                        : theme.colorScheme.surfaceContainerLowest,
                    borderRadius: BorderRadius.circular(10),
                    child: InkWell(
                      borderRadius: BorderRadius.circular(10),
                      onTap: () => _openItem(ref, item),
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(12, 10, 8, 10),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Row(
                              children: [
                                Icon(
                                  isApproval
                                      ? Icons.gavel_outlined
                                      : Icons.error_outline,
                                  size: 16,
                                  color: isApproval
                                      ? KeryxTheme.needsYou
                                      : theme.colorScheme.error,
                                ),
                                const SizedBox(width: 8),
                                Expanded(
                                  child: Text(
                                    item.title,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: theme.textTheme.titleSmall,
                                  ),
                                ),
                                StatusPill(
                                  label: isApproval ? 'Approval' : 'Failed',
                                  tone: isApproval
                                      ? StatusPillTone.attention
                                      : StatusPillTone.danger,
                                ),
                              ],
                            ),
                            const SizedBox(height: 6),
                            Text(
                              item.summary,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              style: theme.textTheme.bodySmall,
                            ),
                            if (isApproval && item.approvalId != null) ...[
                              const SizedBox(height: 10),
                              Row(
                                children: [
                                  TextButton(
                                    onPressed: () =>
                                        _decide(ref, item.approvalId!, false),
                                    child: const Text('Deny'),
                                  ),
                                  const SizedBox(width: 4),
                                  FilledButton(
                                    style: FilledButton.styleFrom(
                                      backgroundColor: KeryxTheme.needsYou,
                                      foregroundColor: const Color(0xFFFFF8F4),
                                    ),
                                    onPressed: () =>
                                        _decide(ref, item.approvalId!, true),
                                    child: const Text('Approve'),
                                  ),
                                  const Spacer(),
                                  if (item.sessionId != null)
                                    TextButton(
                                      onPressed: () => _openItem(ref, item),
                                      child: const Text('Open chat'),
                                    ),
                                ],
                              ),
                            ],
                          ],
                        ),
                      ),
                    ),
                  );
                },
              );
            },
          ),
        ),
      ],
    );
  }

  Future<void> _openItem(WidgetRef ref, InboxItem item) async {
    if (item.sessionId == null) return;
    await ref.read(sessionsControllerProvider.notifier).open(item.sessionId!);
    onOpenSession?.call(item.sessionId!);
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

/// Deprecated alias — dual-rail Inbox rail is superseded by [NeedsYouPane].
@Deprecated('Use NeedsYouPane')
typedef InboxScreen = NeedsYouPane;
