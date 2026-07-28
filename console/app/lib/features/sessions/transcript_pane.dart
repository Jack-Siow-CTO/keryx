import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../artifacts/artifact_viewer.dart';
import '../auth/auth_controller.dart';
import 'session_run_controller.dart';
import 'sessions_controller.dart';

/// Conversation layer from durable Worker Transcript (ADR 0015, 0025).
class TranscriptPane extends ConsumerStatefulWidget {
  const TranscriptPane({super.key, required this.sessionId});

  final String sessionId;

  @override
  ConsumerState<TranscriptPane> createState() => _TranscriptPaneState();
}

class _TranscriptPaneState extends ConsumerState<TranscriptPane> {
  final _scroll = ScrollController();
  final List<TranscriptMessage> _messages = []; // display order: oldest→newest
  String? _nextBefore;
  bool _loading = false;
  String? _error;
  final Set<String> _expandedTools = {};

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
    // Scroll up (near min) loads older history.
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
      // API newest-first → reverse for chronological ListView.
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
      final page =
          await client.getTranscript(widget.sessionId, limit: 50, before: before);
      setState(() {
        // Older messages (page is newest-first among older set) → reverse then prepend.
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

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (_error != null && _messages.isEmpty) {
      return Center(
        child: Text(_error!, style: TextStyle(color: theme.colorScheme.error)),
      );
    }
    if (_loading && _messages.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_messages.isEmpty) {
      return Center(
        child: Text(
          'No Transcript yet. Start a Run from the composer (next slice).',
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
          textAlign: TextAlign.center,
        ),
      );
    }

    return ListView.builder(
      controller: _scroll,
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      itemCount: _messages.length + (_loading ? 1 : 0),
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
        final idx = _loading ? i - 1 : i;
        final m = _messages[idx];
        if (m.isTool) {
          return _ToolRow(
            message: m,
            expanded: _expandedTools.contains(m.id),
            onToggle: () {
              setState(() {
                if (_expandedTools.contains(m.id)) {
                  _expandedTools.remove(m.id);
                } else {
                  _expandedTools.add(m.id);
                }
              });
            },
          );
        }
        return _ProseBubble(message: m);
      },
    );
  }
}

class _ProseBubble extends StatelessWidget {
  const _ProseBubble({required this.message});

  final TranscriptMessage message;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isUser = message.role == 'user';
    final align = isUser ? Alignment.centerRight : Alignment.centerLeft;
    final bg = isUser
        ? theme.colorScheme.primaryContainer
        : theme.colorScheme.surfaceContainerHighest;

    return Align(
      alignment: align,
      child: Container(
        margin: const EdgeInsets.symmetric(vertical: 6),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        constraints: BoxConstraints(
          maxWidth: MediaQuery.sizeOf(context).width * 0.75,
        ),
        decoration: BoxDecoration(
          color: bg,
          borderRadius: BorderRadius.circular(12),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              isUser ? 'You' : message.role,
              style: theme.textTheme.labelSmall?.copyWith(
                fontWeight: FontWeight.w700,
              ),
            ),
            const SizedBox(height: 4),
            SelectableText(message.content),
          ],
        ),
      ),
    );
  }
}

class _ToolRow extends StatelessWidget {
  const _ToolRow({
    required this.message,
    required this.expanded,
    required this.onToggle,
  });

  final TranscriptMessage message;
  final bool expanded;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final tool = message.tool;
    final name = tool?.name ?? 'tool';
    final status = tool?.status ?? '';
    final summary = tool?.summary ?? message.content;

    return Card(
      margin: const EdgeInsets.symmetric(vertical: 4),
      child: InkWell(
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
                  ),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      name,
                      style: theme.textTheme.titleSmall?.copyWith(
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  Text(
                    status,
                    style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.primary,
                    ),
                  ),
                ],
              ),
              if (!expanded)
                Padding(
                  padding: const EdgeInsets.only(left: 24, top: 4),
                  child: Text(
                    summary,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall,
                  ),
                ),
              if (expanded) ...[
                const SizedBox(height: 8),
                Text(summary, style: theme.textTheme.bodySmall),
                if (tool != null && tool.artifactRefs.isNotEmpty) ...[
                  const SizedBox(height: 6),
                  Wrap(
                    spacing: 8,
                    children: [
                      for (final refId in tool.artifactRefs)
                        ActionChip(
                          label: Text(refId.length > 12
                              ? '${refId.substring(0, 12)}…'
                              : refId),
                          onPressed: () {
                            Navigator.of(context).push(
                              MaterialPageRoute<void>(
                                builder: (_) =>
                                    ArtifactViewerPage(artifactId: refId),
                              ),
                            );
                          },
                        ),
                    ],
                  ),
                ],
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// Wire Transcript into Session detail when a Session is selected.
class SessionConversationBody extends ConsumerStatefulWidget {
  const SessionConversationBody({super.key});

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

    // Reload durable Transcript when Run leaves Active (Worker SoR).
    ref.listen(sessionRunControllerProvider, (prev, next) {
      final prevStatus = prev?.activeRun?.status;
      final nextStatus = next.activeRun?.status;
      if (prevStatus == 'active' &&
          nextStatus != null &&
          nextStatus != 'active') {
        _paneKey.currentState?.reloadFromWorker();
      }
    });

    if (selected == null) {
      return const SizedBox.shrink();
    }

    // Skill chips: tools skill_view / skill_load / skills_list in activity
    final skillHints = ref
        .watch(sessionRunControllerProvider)
        .activity
        .where((a) => a.contains('skill'))
        .take(3)
        .toList();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (skillHints.isNotEmpty)
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),
            child: Wrap(
              spacing: 6,
              children: [
                for (final h in skillHints)
                  Chip(
                    label: Text(h, style: const TextStyle(fontSize: 11)),
                    visualDensity: VisualDensity.compact,
                    avatar: const Icon(Icons.extension, size: 14),
                  ),
              ],
            ),
          ),
        Expanded(
          child: TranscriptPane(key: _paneKey, sessionId: selected),
        ),
      ],
    );
  }
}
