import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../widgets/console_chrome.dart';
import '../auth/auth_controller.dart';

/// Shared Artifact body (diff / terminal / screenshot / text).
class _ArtifactBody extends ConsumerStatefulWidget {
  const _ArtifactBody({
    required this.artifactId,
    this.showChromeHeader = false,
    this.onClose,
  });

  final String artifactId;
  final bool showChromeHeader;
  final VoidCallback? onClose;

  @override
  ConsumerState<_ArtifactBody> createState() => _ArtifactBodyState();
}

class _ArtifactBodyState extends ConsumerState<_ArtifactBody> {
  ArtifactMeta? _meta;
  List<int>? _bytes;
  String? _error;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
  }

  @override
  void didUpdateWidget(covariant _ArtifactBody oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.artifactId != widget.artifactId) {
      _meta = null;
      _bytes = null;
      _error = null;
      _load();
    }
  }

  Future<void> _load() async {
    final client = ref.read(authControllerProvider.notifier).client;
    if (client == null) {
      setState(() => _error = 'Not connected');
      return;
    }
    try {
      final meta = await client.getArtifactMeta(widget.artifactId);
      final bytes = await client.getArtifactBytes(widget.artifactId);
      if (!mounted) return;
      setState(() {
        _meta = meta;
        _bytes = bytes;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (_error != null) {
      return ConsoleEmptyState(
        icon: Icons.broken_image_outlined,
        title: 'Could not load Artifact',
        body: _error!,
        action: FilledButton.tonal(
          onPressed: () {
            setState(() {
              _error = null;
              _meta = null;
              _bytes = null;
            });
            _load();
          },
          child: const Text('Retry'),
        ),
      );
    }
    if (_meta == null) {
      return const ConsoleLoader(label: 'Loading Artifact…');
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (widget.showChromeHeader)
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 10, 4, 8),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    _meta?.summary ?? 'Artifact',
                    style: theme.textTheme.titleSmall,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                if (widget.onClose != null)
                  IconButton(
                    tooltip: 'Close',
                    icon: const Icon(Icons.close, size: 20),
                    onPressed: widget.onClose,
                  ),
              ],
            ),
          ),
        if (widget.showChromeHeader)
          Divider(height: 1, color: theme.dividerTheme.color),
        Expanded(
          child: ColoredBox(
            color: theme.colorScheme.surfaceContainerLowest,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
              child: _buildBody(theme),
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildBody(ThemeData theme) {
    final meta = _meta!;
    final bytes = _bytes ?? const <int>[];
    switch (meta.kind) {
      case 'image':
        return Center(
          child: Image.memory(
            Uint8List.fromList(bytes),
            fit: BoxFit.contain,
          ),
        );
      case 'diff':
      case 'terminal':
      case 'text':
      case 'json':
      default:
        final text = utf8.decode(bytes, allowMalformed: true);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                StatusPill(label: meta.kind, tone: StatusPillTone.neutral),
                StatusPill(
                  label: '${meta.byteLen} bytes',
                  tone: StatusPillTone.neutral,
                ),
                StatusPill(
                  label: meta.mediaType,
                  tone: StatusPillTone.neutral,
                ),
              ],
            ),
            const SizedBox(height: 14),
            Expanded(
              child: Material(
                color: theme.colorScheme.surface,
                borderRadius: BorderRadius.circular(10),
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    borderRadius: BorderRadius.circular(10),
                    border: Border.all(
                      color: theme.colorScheme.outlineVariant
                          .withValues(alpha: 0.7),
                    ),
                  ),
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.all(14),
                    child: SelectableText(
                      text,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        fontFamily: 'monospace',
                        fontSize: 13,
                        height: 1.4,
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ],
        );
    }
  }
}

/// Full-screen / push Artifact viewer (narrow layouts).
class ArtifactViewerPage extends StatelessWidget {
  const ArtifactViewerPage({super.key, required this.artifactId});

  final String artifactId;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Artifact')),
      body: _ArtifactBody(artifactId: artifactId),
    );
  }
}

/// Contextual third-pane Artifact viewer (wide messaging shell — ticket #63).
/// Does not reintroduce permanent dual-rail cockpit chrome.
class ArtifactViewerPane extends StatelessWidget {
  const ArtifactViewerPane({
    super.key,
    required this.artifactId,
    this.onClose,
  });

  final String artifactId;
  final VoidCallback? onClose;

  @override
  Widget build(BuildContext context) {
    return _ArtifactBody(
      artifactId: artifactId,
      showChromeHeader: true,
      onClose: onClose,
    );
  }
}
