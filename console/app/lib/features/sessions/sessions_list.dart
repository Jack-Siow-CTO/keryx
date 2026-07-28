import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../theme/keryx_theme.dart';
import 'sessions_controller.dart';

/// Sessions rail: channel-style rows (ADR 0014 / 0027).
class SessionsList extends ConsumerStatefulWidget {
  const SessionsList({
    super.key,
    this.onOpenSession,
  });

  /// When set (narrow stack), called after a Session is selected so the shell
  /// can push full-screen detail.
  final ValueChanged<String>? onOpenSession;

  @override
  ConsumerState<SessionsList> createState() => _SessionsListState();
}

class _SessionsListState extends ConsumerState<SessionsList> {
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
                  'Sessions',
                  style: theme.textTheme.titleSmall?.copyWith(
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ),
              IconButton(
                tooltip: 'Refresh',
                icon: const Icon(Icons.refresh, size: 20),
                onPressed: state.loading
                    ? null
                    : () => ref
                        .read(sessionsControllerProvider.notifier)
                        .refresh(),
              ),
              IconButton(
                tooltip: 'New Session',
                icon: const Icon(Icons.add, size: 20),
                onPressed: () async {
                  final created = await ref
                      .read(sessionsControllerProvider.notifier)
                      .createSession();
                  if (created != null) {
                    widget.onOpenSession?.call(created.id);
                  }
                },
              ),
            ],
          ),
        ),
        if (state.error != null)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: Text(
              state.error!,
              style: TextStyle(color: theme.colorScheme.error, fontSize: 12),
            ),
          ),
        Expanded(
          child: state.loading && state.sessions.isEmpty
              ? const Center(child: CircularProgressIndicator())
              : state.sessions.isEmpty
                  ? Center(
                      child: Padding(
                        padding: const EdgeInsets.all(16),
                        child: Text(
                          'No Sessions yet. Create one to start a channel.',
                          textAlign: TextAlign.center,
                          style: theme.textTheme.bodyMedium?.copyWith(
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ),
                      ),
                    )
                  : ListView.builder(
                      itemCount: state.sessions.length,
                      itemBuilder: (context, i) {
                        final s = state.sessions[i];
                        final selected = s.id == state.selectedId;
                        return SessionRow(
                          session: s,
                          selected: selected,
                          onTap: () async {
                            await ref
                                .read(sessionsControllerProvider.notifier)
                                .open(s.id);
                            widget.onOpenSession?.call(s.id);
                          },
                          onRename: () => _renameDialog(s),
                        );
                      },
                    ),
        ),
      ],
    );
  }

  Future<void> _renameDialog(SessionSummary session) async {
    final controller = TextEditingController(text: session.title);
    final name = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Rename Session'),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(labelText: 'Title'),
          onSubmitted: (v) => Navigator.pop(ctx, v),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, controller.text),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    if (name != null && name.trim().isNotEmpty) {
      await ref
          .read(sessionsControllerProvider.notifier)
          .rename(session.id, name.trim());
    }
  }
}

class SessionRow extends StatelessWidget {
  const SessionRow({
    super.key,
    required this.session,
    required this.selected,
    required this.onTap,
    required this.onRename,
  });

  final SessionSummary session;
  final bool selected;
  final VoidCallback onTap;
  final VoidCallback onRename;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final hasAttention = session.pendingApprovalCount > 0 ||
        session.activeRootRun != null;

    return ListTile(
      selected: selected,
      dense: true,
      title: Row(
        children: [
          Expanded(
            child: Text(
              session.title,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(fontWeight: FontWeight.w600),
            ),
          ),
          if (session.pendingApprovalCount > 0)
            Container(
              margin: const EdgeInsets.only(left: 6),
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              decoration: BoxDecoration(
                color: KeryxTheme.needsYou,
                borderRadius: BorderRadius.circular(10),
              ),
              child: Text(
                '${session.pendingApprovalCount}',
                style: const TextStyle(
                  color: Colors.white,
                  fontSize: 11,
                  fontWeight: FontWeight.w700,
                ),
              ),
            )
          else if (hasAttention)
            Container(
              margin: const EdgeInsets.only(left: 6),
              width: 8,
              height: 8,
              decoration: const BoxDecoration(
                color: KeryxTheme.needsYou,
                shape: BoxShape.circle,
              ),
            ),
        ],
      ),
      subtitle: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (session.lastMessagePreview != null)
            Text(
              session.lastMessagePreview!,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: theme.textTheme.bodySmall,
            ),
          if (session.activeRootRun != null)
            Text(
              'Active · ${session.activeRootRun!.goal}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.primary,
              ),
            ),
        ],
      ),
      onTap: onTap,
      onLongPress: onRename,
      trailing: IconButton(
        icon: const Icon(Icons.edit_outlined, size: 18),
        tooltip: 'Rename',
        onPressed: onRename,
      ),
    );
  }
}
