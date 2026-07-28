import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../auth/auth_controller.dart';

class SettingsScreen extends ConsumerStatefulWidget {
  const SettingsScreen({super.key});

  @override
  ConsumerState<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends ConsumerState<SettingsScreen> {
  final _urlController = TextEditingController();
  final _tokenController = TextEditingController();
  bool _obscure = true;
  bool _busy = false;
  bool _checking = false;

  @override
  void initState() {
    super.initState();
    final auth = ref.read(authControllerProvider);
    _urlController.text = auth.baseUrl ?? 'http://127.0.0.1:8787';
  }

  @override
  void dispose() {
    _urlController.dispose();
    _tokenController.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    setState(() => _busy = true);
    try {
      final token = _tokenController.text.trim();
      final auth = ref.read(authControllerProvider);
      final notifier = ref.read(authControllerProvider.notifier);

      final ConnectivityResult? result;
      if (token.isEmpty) {
        // URL-only (or biometric already handled by switch): keep existing token.
        result = await notifier.updateBaseUrl(_urlController.text);
      } else {
        result = await notifier.saveConnection(
          baseUrl: _urlController.text,
          operatorToken: token,
          biometricLockEnabled: auth.biometricLockEnabled,
          probe: true,
        );
      }

      if (!mounted) return;
      if (result == null) {
        // Validation error already on state.
      } else if (result.isOk) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Saved to secure storage')),
        );
        _tokenController.clear();
      } else {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(result.message)),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _health() async {
    setState(() => _checking = true);
    try {
      await ref.read(authControllerProvider.notifier).checkHealth();
    } finally {
      if (mounted) setState(() => _checking = false);
    }
  }

  Future<void> _logout() async {
    await ref.read(authControllerProvider.notifier).logout();
    if (mounted) Navigator.of(context).popUntil((r) => r.isFirst);
  }

  @override
  Widget build(BuildContext context) {
    final auth = ref.watch(authControllerProvider);
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text('Connection', style: theme.textTheme.titleMedium),
          const SizedBox(height: 12),
          TextField(
            controller: _urlController,
            decoration: const InputDecoration(
              labelText: 'Worker base URL',
            ),
            autocorrect: false,
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _tokenController,
            decoration: InputDecoration(
              labelText: 'Operator token',
              hintText: 'Leave blank to keep existing',
              helperText:
                  'Stored in OS secure storage (Keychain/Keystore) only — never plaintext prefs',
              suffixIcon: IconButton(
                icon: Icon(_obscure ? Icons.visibility : Icons.visibility_off),
                onPressed: () => setState(() => _obscure = !_obscure),
              ),
            ),
            obscureText: _obscure,
            autocorrect: false,
            enableSuggestions: false,
          ),
          const SizedBox(height: 8),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Device unlock gate'),
            subtitle: const Text('Biometric or device credential on open'),
            value: auth.biometricLockEnabled,
            onChanged: (v) {
              ref
                  .read(authControllerProvider.notifier)
                  .setBiometricLockEnabled(v);
            },
          ),
          const SizedBox(height: 8),
          Row(
            children: [
              FilledButton(
                onPressed: _busy ? null : _save,
                child: const Text('Save'),
              ),
              const SizedBox(width: 12),
              OutlinedButton.icon(
                onPressed: _checking ? null : _health,
                icon: _checking
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.health_and_safety_outlined),
                label: const Text('Check connectivity'),
              ),
            ],
          ),
          if (auth.lastConnectivity != null) ...[
            const SizedBox(height: 16),
            _ConnectivityBanner(result: auth.lastConnectivity!),
          ],
          if (auth.errorMessage != null) ...[
            const SizedBox(height: 12),
            Text(
              auth.errorMessage!,
              style: TextStyle(color: theme.colorScheme.error),
            ),
          ],
          const Divider(height: 40),
          Text('Session', style: theme.textTheme.titleMedium),
          const SizedBox(height: 8),
          Text(
            'Logout deletes the operator token from secure storage and clears local caches. '
            'No offline Start Run queue exists.',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
          const SizedBox(height: 12),
          OutlinedButton.icon(
            onPressed: _logout,
            icon: const Icon(Icons.logout),
            label: const Text('Log out'),
            style: OutlinedButton.styleFrom(
              foregroundColor: theme.colorScheme.error,
            ),
          ),
        ],
      ),
    );
  }
}

class _ConnectivityBanner extends StatelessWidget {
  const _ConnectivityBanner({required this.result});

  final ConnectivityResult result;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final (Color bg, IconData icon, String title) = switch (result.kind) {
      ConnectivityKind.ok => (
          theme.colorScheme.primaryContainer,
          Icons.check_circle_outline,
          'Connected',
        ),
      ConnectivityKind.unreachable => (
          theme.colorScheme.errorContainer,
          Icons.cloud_off_outlined,
          'Worker unreachable',
        ),
      ConnectivityKind.authFailure => (
          theme.colorScheme.tertiaryContainer,
          Icons.lock_outline,
          'Authentication failed',
        ),
      ConnectivityKind.unexpected => (
          theme.colorScheme.surfaceContainerHighest,
          Icons.warning_amber_outlined,
          'Unexpected response',
        ),
    };

    return Material(
      color: bg,
      borderRadius: BorderRadius.circular(8),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(icon, size: 20),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(title, style: theme.textTheme.titleSmall),
                  const SizedBox(height: 4),
                  Text(result.message, style: theme.textTheme.bodySmall),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
