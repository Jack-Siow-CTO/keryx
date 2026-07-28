import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../auth/auth_controller.dart';

/// Basic Artifact viewers: terminal/text/diff/json/image (ADR 0026).
class ArtifactViewerPage extends ConsumerStatefulWidget {
  const ArtifactViewerPage({super.key, required this.artifactId});

  final String artifactId;

  @override
  ConsumerState<ArtifactViewerPage> createState() => _ArtifactViewerPageState();
}

class _ArtifactViewerPageState extends ConsumerState<ArtifactViewerPage> {
  ArtifactMeta? _meta;
  List<int>? _bytes;
  String? _error;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
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
    return Scaffold(
      appBar: AppBar(
        title: Text(_meta?.summary ?? 'Artifact'),
      ),
      body: _error != null
          ? Center(child: Text(_error!))
          : _meta == null
              ? const Center(child: CircularProgressIndicator())
              : Padding(
                  padding: const EdgeInsets.all(16),
                  child: _buildBody(theme),
                ),
    );
  }

  Widget _buildBody(ThemeData theme) {
    final meta = _meta!;
    final bytes = _bytes ?? const <int>[];
    switch (meta.kind) {
      case 'image':
        return Image.memory(Uint8List.fromList(bytes), fit: BoxFit.contain);
      case 'diff':
      case 'terminal':
      case 'text':
      case 'json':
      default:
        final text = utf8.decode(bytes, allowMalformed: true);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              '${meta.kind} · ${meta.byteLen} bytes · ${meta.mediaType}',
              style: theme.textTheme.labelSmall,
            ),
            const SizedBox(height: 12),
            Expanded(
              child: SingleChildScrollView(
                child: SelectableText(
                  text,
                  style: const TextStyle(fontFamily: 'monospace', fontSize: 13),
                ),
              ),
            ),
          ],
        );
    }
  }
}
