import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../theme/keryx_theme.dart';
import '../auth/auth_controller.dart';
import '../sessions/sessions_controller.dart';

final inboxProvider = FutureProvider.autoDispose<List<InboxItem>>((ref) async {
  final client = ref.watch(authControllerProvider.notifier).client;
  if (client == null) return const [];
  return client.listInbox();
});

/// Inbox rail: control-plane projection of Approvals + failed Runs (ADR 0028).
class InboxScreen extends ConsumerWidget {
  const InboxScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final async = ref.watch(inboxProvider);
    final theme = Theme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 8, 8, 4),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  'Inbox',
                  style: theme.textTheme.titleSmall?.copyWith(
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ),
              IconButton(
                icon: const Icon(Icons.refresh, size: 20),
                onPressed: () => ref.invalidate(inboxProvider),
              ),
            ],
          ),
        ),
        Expanded(
          child: async.when(
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (e, _) => Center(child: Text('$e')),
            data: (items) {
              if (items.isEmpty) {
                return Center(
                  child: Padding(
                    padding: const EdgeInsets.all(16),
                    child: Text(
                      'Nothing needs you. Pending Approvals and failed Runs appear here.',
                      textAlign: TextAlign.center,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  ),
                );
              }
              return ListView.builder(
                itemCount: items.length,
                itemBuilder: (context, i) {
                  final item = items[i];
                  return ListTile(
                    leading: Icon(
                      item.kind == 'approval_pending'
                          ? Icons.gavel
                          : Icons.error_outline,
                      color: KeryxTheme.needsYou,
                    ),
                    title: Text(item.title),
                    subtitle: Text(item.summary, maxLines: 2),
                    trailing: item.kind == 'approval_pending' &&
                            item.approvalId != null
                        ? Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              IconButton(
                                tooltip: 'Approve',
                                icon: const Icon(Icons.check_circle_outline),
                                onPressed: () => _decide(ref, item.approvalId!, true),
                              ),
                              IconButton(
                                tooltip: 'Deny',
                                icon: const Icon(Icons.cancel_outlined),
                                onPressed: () => _decide(ref, item.approvalId!, false),
                              ),
                            ],
                          )
                        : null,
                    onTap: () {
                      if (item.sessionId != null) {
                        ref
                            .read(sessionsControllerProvider.notifier)
                            .open(item.sessionId!);
                      }
                    },
                  );
                },
              );
            },
          ),
        ),
      ],
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
