import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../widgets/console_chrome.dart';
import '../auth/auth_controller.dart';
import 'session_run_controller.dart';
import 'sessions_controller.dart';

/// Layered thread timeline from durable Worker Transcript (ADR 0015).
///
/// Prose = first-class messages; tools/Child-Run/status = collapsible activity.
/// Live Run activity (same shapes) sits after durable rows while a Run is open.
/// Not flat bubble spam of every event; not default Chat | Activity tabs.
class TranscriptPane extends ConsumerStatefulWidget {
  const TranscriptPane({
    super.key,
    required this.sessionId,
    this.onOpenArtifact,
  });

  final String sessionId;
  final ValueChanged<String>? onOpenArtifact;

  @override
  ConsumerState<TranscriptPane> createState() => _TranscriptPaneState();
}

class _TranscriptPaneState extends ConsumerState<TranscriptPane> {
  final _scroll = ScrollController();
  final List<TranscriptMessage> _messages = []; // display order: oldest→newest
  String? _nextBefore;
  bool _loading = false;
  String? _error;
  final Set<String> _expanded = {};

  @override
  void initState() {
    super.initState();
    _scroll.addListener(_onScroll);
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadInitial());
  }

  @override
  void didUpdateWidget(covariant TranscriptPane oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.sessionId != widget.sessionId) {
      _messages.clear();
      _nextBefore = null;
      _expanded.clear();
      _loadInitial();
    }
  }

  /// Public reload for Run terminal / reconnect (Worker Transcript SoR).
  Future<void> reloadFromWorker() => _loadInitial();

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  void _onScroll() {
    if (_scroll.position.pixels <= 40 &&
        _nextBefore != null &&
        !_loading) {
      _loadMore();
    }
  }

  Future<void> _loadInitial() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final client = ref.read(authControllerProvider.notifier).client;
      if (client == null) throw Exception('Not connected');
      final page = await client.getTranscript(widget.sessionId, limit: 50);
      setState(() {
        _messages
          ..clear()
          ..addAll(page.messages.reversed);
        _nextBefore = page.nextBefore;
        _loading = false;
      });
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_scroll.hasClients) {
          _scroll.jumpTo(_scroll.position.maxScrollExtent);
        }
      });
    } catch (e) {
      setState(() {
        _loading = false;
        _error = e.toString();
      });
    }
  }

  Future<void> _loadMore() async {
    final before = _nextBefore;
    if (before == null) return;
    setState(() => _loading = true);
    try {
      final client = ref.read(authControllerProvider.notifier).client;
      if (client == null) return;
      final page = await client.getTranscript(
        widget.sessionId,
        limit: 50,
        before: before,
      );
      setState(() {
        _messages.insertAll(0, page.messages.reversed);
        _nextBefore = page.nextBefore;
        _loading = false;
      });
    } catch (e) {
      setState(() {
        _loading = false;
        _error = e.toString();
      });
    }
  }

  void _toggleExpanded(String id) {
    setState(() {
      if (_expanded.contains(id)) {
        _expanded.remove(id);
      } else {
        _expanded.add(id);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final runState = ref.watch(sessionRunControllerProvider);
    final liveForSession = runState.boundSessionId == widget.sessionId
        ? runState.liveActivity
        : const <LiveActivityItem>[];

    if (_error != null && _messages.isEmpty) {
      return ConsoleEmptyState(
        icon: Icons.error_outline,
        title: 'Transcript unavailable',
        body: _error!,
        action: FilledButton.tonal(
          onPressed: _loadInitial,
          child: const Text('Retry'),
        ),
      );
    }
    if (_loading && _messages.isEmpty && liveForSession.isEmpty) {
      return const ConsoleLoader(label: 'Loading Transcript…');
    }
    if (_messages.isEmpty && liveForSession.isEmpty) {
      return const ConsoleEmptyState(
        icon: Icons.chat_bubble_outline,
        title: 'No messages yet',
        body:
            'Send a message to start the first Run. Prose appears as chat; tools stay collapsible activity.',
      );
    }

    final loadingHeader = _loading ? 1 : 0;
    final durableCount = _messages.length;
    final liveCount = liveForSession.length;
    final itemCount = loadingHeader + durableCount + liveCount;

    return ListView.builder(
      controller: _scroll,
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
      itemCount: itemCount,
      itemBuilder: (context, i) {
        if (_loading && i == 0) {
          return const Padding(
            padding: EdgeInsets.all(8),
            child: Center(
              child: SizedBox(
                width: 18,
                height: 18,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
            ),
          );
        }
        final idx = i - loadingHeader;
        if (idx < durableCount) {
          final m = _messages[idx];
          if (m.isTool) {
            return ActivityBlock(
              key: ValueKey('durable-${m.id}'),
              blockId: m.id,
              title: m.tool?.name ?? 'tool',
              status: m.tool?.status ?? '',
              summary: m.tool?.summary ?? m.content,
              looksLikeChild: _looksLikeChild(
                m.tool?.name ?? '',
                m.tool?.summary ?? m.content,
              ),
              expanded: _expanded.contains(m.id),
              onToggle: () => _toggleExpanded(m.id),
              artifactRefs: m.tool?.artifactRefs ?? const [],
              onOpenArtifact: widget.onOpenArtifact,
            );
          }
          return _ProseMessage(message: m);
        }
        final live = liveForSession[idx - durableCount];
        return ActivityBlock(
          key: ValueKey('live-${live.id}'),
          blockId: live.id,
          title: live.title,
          status: live.status,
          summary: live.summary,
          looksLikeChild: live.looksLikeChild,
          expanded: _expanded.contains(live.id),
          onToggle: () => _toggleExpanded(live.id),
          live: true,
        );
      },
    );
  }

  bool _looksLikeChild(String name, String summary) {
    return name.toLowerCase().contains('child') ||
        summary.toLowerCase().contains('child run');
  }
}

/// Operator-readable prose row (not mandatory consumer chat bubble cosplay).
class _ProseMessage extends StatelessWidget {
  const _ProseMessage({required this.message});

  final TranscriptMessage message;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isUser = message.role == 'user';
    final isSystem = message.role == 'system';
    final author = isUser
        ? 'You'
        : isSystem
            ? 'System'
            : 'Keryx';
    final authorColor = isUser
        ? theme.colorScheme.primary
        : theme.colorScheme.onSurfaceVariant;

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Text(
                author,
                style: theme.textTheme.labelSmall?.copyWith(
                  fontWeight: FontWeight.w700,
                  color: authorColor,
                ),
              ),
              if (message.createdAt > 0) ...[
                const SizedBox(width: 8),
                Text(
                  _formatTime(message.createdAt),
                  style: theme.textTheme.labelSmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ],
            ],
          ),
          const SizedBox(height: 4),
          SelectableText(
            message.content,
            style: theme.textTheme.bodyMedium?.copyWith(height: 1.45),
          ),
        ],
      ),
    );
  }

  String _formatTime(int epochSecs) {
    final dt = DateTime.fromMillisecondsSinceEpoch(epochSecs * 1000);
    final h = dt.hour.toString().padLeft(2, '0');
    final m = dt.minute.toString().padLeft(2, '0');
    return '$h:$m';
  }
}

/// Collapsible tool / Child-Run / status activity in the same timeline.
///
/// Used for durable Transcript tool rows and live Run activity (#75).
class ActivityBlock extends StatelessWidget {
  const ActivityBlock({
    super.key,
    required this.blockId,
    required this.title,
    required this.status,
    required this.summary,
    required this.looksLikeChild,
    required this.expanded,
    required this.onToggle,
    this.artifactRefs = const [],
    this.onOpenArtifact,
    this.live = false,
  });

  final String blockId;
  final String title;
  final String status;
  final String summary;
  final bool looksLikeChild;
  final bool expanded;
  final VoidCallback onToggle;
  final List<String> artifactRefs;
  final ValueChanged<String>? onOpenArtifact;
  final bool live;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Material(
        color: live
            ? theme.colorScheme.primaryContainer.withValues(alpha: 0.35)
            : theme.colorScheme.surfaceContainerLow,
        borderRadius: BorderRadius.circular(10),
        child: InkWell(
          borderRadius: BorderRadius.circular(10),
          onTap: onToggle,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Icon(
                      expanded ? Icons.expand_more : Icons.chevron_right,
                      size: 18,
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                    const SizedBox(width: 6),
                    Icon(
                      looksLikeChild
                          ? Icons.account_tree_outlined
                          : Icons.build_circle_outlined,
                      size: 15,
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                    const SizedBox(width: 6),
                    Expanded(
                      child: Text(
                        title,
                        style: theme.textTheme.titleSmall,
                      ),
                    ),
                    if (live) ...[
                      Text(
                        'live',
                        style: theme.textTheme.labelSmall?.copyWith(
                          color: theme.colorScheme.primary,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      const SizedBox(width: 8),
                    ],
                    Text(
                      status,
                      style: theme.textTheme.labelSmall?.copyWith(
                        color: theme.colorScheme.primary,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ],
                ),
                if (!expanded)
                  Padding(
                    padding: const EdgeInsets.only(left: 45, top: 3),
                    child: Text(
                      summary,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall,
                    ),
                  ),
                if (expanded) ...[
                  const SizedBox(height: 8),
                  Padding(
                    padding: const EdgeInsets.only(left: 45),
                    child: Text(
                      summary,
                      style: theme.textTheme.bodySmall?.copyWith(
                        fontFamily: 'monospace',
                        fontSize: 12,
                      ),
                    ),
                  ),
                  if (looksLikeChild) ...[
                    const SizedBox(height: 6),
                    Padding(
                      padding: const EdgeInsets.only(left: 45),
                      child: Text(
                        'Child Run (read-only linkage — not a separate chat)',
                        style: theme.textTheme.labelSmall?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ),
                  ],
                  if (artifactRefs.isNotEmpty) ...[
                    const SizedBox(height: 8),
                    Padding(
                      padding: const EdgeInsets.only(left: 45),
                      child: Wrap(
                        spacing: 8,
                        children: [
                          for (final refId in artifactRefs)
                            ActionChip(
                              avatar: const Icon(Icons.attach_file, size: 14),
                              label: Text(
                                refId.length > 12
                                    ? '${refId.substring(0, 12)}…'
                                    : refId,
                              ),
                              onPressed: () {
                                if (onOpenArtifact != null) {
                                  onOpenArtifact!(refId);
                                }
                              },
                            ),
                        ],
                      ),
                    ),
                  ],
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Wire Transcript into Session detail when a Session is selected.
class SessionConversationBody extends ConsumerStatefulWidget {
  const SessionConversationBody({super.key, this.onOpenArtifact});

  final ValueChanged<String>? onOpenArtifact;

  @override
  ConsumerState<SessionConversationBody> createState() =>
      _SessionConversationBodyState();
}

class _SessionConversationBodyState
    extends ConsumerState<SessionConversationBody> {
  final _paneKey = GlobalKey<_TranscriptPaneState>();

  @override
  Widget build(BuildContext context) {
    final selected = ref.watch(sessionsControllerProvider).selectedId;

    ref.listen(sessionRunControllerProvider, (prev, next) {
      final prevStatus = prev?.activeRun?.status;
      final nextStatus = next.activeRun?.status;
      if (prevStatus == 'active' &&
          nextStatus != null &&
          nextStatus != 'active') {
        _paneKey.currentState?.reloadFromWorker();
      }
      // Resume / SSE reconnect: reload durable Transcript from Worker.
      if (prev != null && next.reconnectEpoch > prev.reconnectEpoch) {
        _paneKey.currentState?.reloadFromWorker();
      }
    });

    if (selected == null) {
      return const SizedBox.shrink();
    }

    return TranscriptPane(
      key: _paneKey,
      sessionId: selected,
      onOpenArtifact: widget.onOpenArtifact,
    );
  }
}
