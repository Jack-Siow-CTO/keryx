import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../widgets/console_chrome.dart';
import '../auth/auth_controller.dart';

class MemoryScreen extends ConsumerStatefulWidget {
  const MemoryScreen({super.key});

  @override
  ConsumerState<MemoryScreen> createState() => _MemoryScreenState();
}

class _MemoryScreenState extends ConsumerState<MemoryScreen> {
  final _search = TextEditingController();
  final _content = TextEditingController();
  final _label = TextEditingController();
  List<MemoryEntry> _entries = const [];
  bool _loading = true;
  String? _error;
  String? _editingId;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _reload());
  }

  @override
  void dispose() {
    _search.dispose();
    _content.dispose();
    _label.dispose();
    super.dispose();
  }

  Future<void> _reload({String? query}) async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final client = ref.read(authControllerProvider.notifier).client;
      if (client == null) throw Exception('Not connected');
      final entries = await client.listMemory(query: query);
      if (!mounted) return;
      setState(() {
        _entries = entries;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = e.toString();
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return ConsolePageScaffold(
      title: 'Memory',
      actions: [
        IconButton(
          tooltip: 'Refresh',
          icon: const Icon(Icons.refresh, size: 20),
          onPressed: _loading
              ? null
              : () => _reload(
                    query: _search.text.trim().isEmpty
                        ? null
                        : _search.text.trim(),
                  ),
        ),
      ],
      body: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
            child: TextField(
              controller: _search,
              decoration: InputDecoration(
                hintText: 'Search Memory',
                prefixIcon: const Icon(Icons.search, size: 20),
                suffixIcon: _search.text.isEmpty
                    ? null
                    : IconButton(
                        icon: const Icon(Icons.clear, size: 18),
                        onPressed: () {
                          _search.clear();
                          _reload();
                        },
                      ),
              ),
              onChanged: (_) => setState(() {}),
              onSubmitted: (q) =>
                  _reload(query: q.trim().isEmpty ? null : q.trim()),
            ),
          ),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
              child: ConsoleBanner(message: _error!),
            ),
          Expanded(
            child: _loading
                ? const ConsoleLoader(label: 'Loading Memory…')
                : _entries.isEmpty
                    ? ConsoleEmptyState(
                        icon: Icons.memory_outlined,
                        title: 'No Memory entries',
                        body:
                            'Add durable notes the Worker can use across Sessions. Search scopes the list.',
                        action: TextButton(
                          onPressed: () {},
                          child: Text(
                            'Write below to add',
                            style: theme.textTheme.labelLarge?.copyWith(
                              color: theme.colorScheme.primary,
                            ),
                          ),
                        ),
                      )
                    : ListView.builder(
                        padding: const EdgeInsets.only(bottom: 12),
                        itemCount: _entries.length,
                        itemBuilder: (context, i) {
                          final e = _entries[i];
                          final selected = e.id == _editingId;
                          return ConsoleListRow(
                            selected: selected,
                            title: e.label?.isNotEmpty == true
                                ? e.label!
                                : e.id,
                            subtitle: e.content,
                            trailing: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                IconButton(
                                  tooltip: 'Edit',
                                  icon: const Icon(Icons.edit_outlined, size: 18),
                                  visualDensity: VisualDensity.compact,
                                  onPressed: () {
                                    setState(() {
                                      _editingId = e.id;
                                      _content.text = e.content;
                                      _label.text = e.label ?? '';
                                    });
                                  },
                                ),
                                IconButton(
                                  tooltip: 'Delete',
                                  icon: const Icon(Icons.delete_outline, size: 18),
                                  visualDensity: VisualDensity.compact,
                                  onPressed: () async {
                                    final client = ref
                                        .read(authControllerProvider.notifier)
                                        .client;
                                    await client?.deleteMemory(e.id);
                                    await _reload(
                                      query: _search.text.trim().isEmpty
                                          ? null
                                          : _search.text.trim(),
                                    );
                                  },
                                ),
                              ],
                            ),
                          );
                        },
                      ),
          ),
        ],
      ),
      bottom: Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            if (_editingId != null)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Row(
                  children: [
                    const StatusPill(
                      label: 'Editing entry',
                      tone: StatusPillTone.active,
                      icon: Icons.edit_outlined,
                    ),
                    const Spacer(),
                    TextButton(
                      onPressed: () {
                        setState(() {
                          _editingId = null;
                          _content.clear();
                          _label.clear();
                        });
                      },
                      child: const Text('Cancel'),
                    ),
                  ],
                ),
              ),
            TextField(
              controller: _label,
              decoration: const InputDecoration(labelText: 'Label (optional)'),
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _content,
              decoration: const InputDecoration(labelText: 'Content'),
              minLines: 2,
              maxLines: 4,
            ),
            const SizedBox(height: 12),
            Align(
              alignment: Alignment.centerRight,
              child: FilledButton(
                onPressed: _save,
                child: Text(
                  _editingId == null ? 'Add Memory' : 'Update Memory',
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _save() async {
    final client = ref.read(authControllerProvider.notifier).client;
    if (client == null) return;
    final content = _content.text.trim();
    if (content.isEmpty) return;
    final label = _label.text.trim().isEmpty ? null : _label.text.trim();
    if (_editingId == null) {
      await client.createMemory(content: content, label: label);
    } else {
      await client.updateMemory(_editingId!, content: content, label: label);
    }
    _content.clear();
    _label.clear();
    setState(() => _editingId = null);
    await _reload(
      query: _search.text.trim().isEmpty ? null : _search.text.trim(),
    );
  }
}
