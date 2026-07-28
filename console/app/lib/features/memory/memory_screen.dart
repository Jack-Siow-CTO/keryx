import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

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
    return Scaffold(
      appBar: AppBar(title: const Text('Memory')),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(12),
            child: TextField(
              controller: _search,
              decoration: InputDecoration(
                labelText: 'Search Memory',
                suffixIcon: IconButton(
                  icon: const Icon(Icons.search),
                  onPressed: () => _reload(
                    query: _search.text.trim().isEmpty
                        ? null
                        : _search.text.trim(),
                  ),
                ),
              ),
              onSubmitted: (q) =>
                  _reload(query: q.trim().isEmpty ? null : q.trim()),
            ),
          ),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.all(8),
              child: Text(_error!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
            ),
          Expanded(
            child: _loading
                ? const Center(child: CircularProgressIndicator())
                : ListView.builder(
                    itemCount: _entries.length,
                    itemBuilder: (context, i) {
                      final e = _entries[i];
                      return ListTile(
                        title: Text(e.label ?? e.id),
                        subtitle: Text(e.content, maxLines: 3),
                        trailing: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            IconButton(
                              icon: const Icon(Icons.edit_outlined),
                              onPressed: () {
                                setState(() {
                                  _editingId = e.id;
                                  _content.text = e.content;
                                  _label.text = e.label ?? '';
                                });
                              },
                            ),
                            IconButton(
                              icon: const Icon(Icons.delete_outline),
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
          const Divider(height: 1),
          Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              children: [
                TextField(
                  controller: _label,
                  decoration: const InputDecoration(labelText: 'Label (optional)'),
                ),
                const SizedBox(height: 8),
                TextField(
                  controller: _content,
                  decoration: const InputDecoration(labelText: 'Content'),
                  minLines: 2,
                  maxLines: 4,
                ),
                const SizedBox(height: 8),
                Align(
                  alignment: Alignment.centerRight,
                  child: FilledButton(
                    onPressed: _save,
                    child: Text(_editingId == null ? 'Add Memory' : 'Update Memory'),
                  ),
                ),
              ],
            ),
          ),
        ],
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
