import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../core/run_preferences.dart';
import '../../widgets/console_chrome.dart';
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
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(runPreferencesProvider.notifier).refresh();
    });
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
        await ref.read(runPreferencesProvider.notifier).refresh();
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
    final runPrefs = ref.watch(runPreferencesProvider);
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(20, 16, 20, 32),
        children: [
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 560),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const ConsoleSectionLabel('Connection'),
                Text(
                  'Worker base URL and operator token. Token stays in Keychain or Keystore only. '
                  'On jack-agent-worker use the Tailnet Edge URL with port :8443 (not host/T3 :443).',
                  style: theme.textTheme.bodySmall,
                ),
                const SizedBox(height: 16),
                TextField(
                  controller: _urlController,
                  decoration: const InputDecoration(
                    labelText: 'Worker base URL',
                    hintText: 'https://host.tailnet.ts.net:8443',
                    helperText: 'Edge :8443 — not host :443',
                  ),
                  autocorrect: false,
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _tokenController,
                  decoration: InputDecoration(
                    labelText: 'Operator token',
                    hintText: 'Leave blank to keep existing',
                    suffixIcon: IconButton(
                      icon: Icon(
                        _obscure
                            ? Icons.visibility_outlined
                            : Icons.visibility_off_outlined,
                      ),
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
                  subtitle: Text(
                    'Biometric or device credential on open',
                    style: theme.textTheme.bodySmall,
                  ),
                  value: auth.biometricLockEnabled,
                  onChanged: (v) {
                    ref
                        .read(authControllerProvider.notifier)
                        .setBiometricLockEnabled(v);
                  },
                ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 10,
                  runSpacing: 10,
                  children: [
                    FilledButton(
                      onPressed: _busy ? null : _save,
                      child: _busy
                          ? const SizedBox(
                              width: 16,
                              height: 16,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Text('Save'),
                    ),
                    OutlinedButton.icon(
                      onPressed: _checking ? null : _health,
                      icon: _checking
                          ? const SizedBox(
                              width: 16,
                              height: 16,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Icon(
                              Icons.health_and_safety_outlined,
                              size: 18,
                            ),
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
                  ConsoleBanner(message: auth.errorMessage!),
                ],
                const SizedBox(height: 32),
                Divider(color: theme.dividerTheme.color),
                const SizedBox(height: 24),
                const ConsoleSectionLabel('Default model'),
                Text(
                  'Used for every new Run. Change once here instead of per Session.',
                  style: theme.textTheme.bodySmall,
                ),
                const SizedBox(height: 16),
                if (runPrefs.loading && runPrefs.providers.isEmpty)
                  const Padding(
                    padding: EdgeInsets.symmetric(vertical: 12),
                    child: ConsoleLoader(label: 'Loading providers…'),
                  )
                else if (runPrefs.providers.isEmpty) ...[
                  ConsoleBanner(
                    message: runPrefs.error ??
                        'No registered providers on the Worker. Configure models on the Worker, then refresh.',
                    tone: StatusPillTone.neutral,
                  ),
                  const SizedBox(height: 10),
                  Align(
                    alignment: Alignment.centerLeft,
                    child: OutlinedButton.icon(
                      onPressed: () =>
                          ref.read(runPreferencesProvider.notifier).refresh(),
                      icon: const Icon(Icons.refresh, size: 18),
                      label: const Text('Refresh providers'),
                    ),
                  ),
                ] else ...[
                  DropdownButtonFormField<String>(
                    // ignore: deprecated_member_use
                    value: runPrefs.provider,
                    isExpanded: true,
                    decoration: const InputDecoration(labelText: 'Provider'),
                    items: runPrefs.providers
                        .map(
                          (p) => DropdownMenuItem(
                            value: p.name,
                            child: Text(
                              p.displayName,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                        )
                        .toList(),
                    onChanged: (v) =>
                        ref.read(runPreferencesProvider.notifier).setProvider(v),
                  ),
                  const SizedBox(height: 12),
                  DropdownButtonFormField<String>(
                    // ignore: deprecated_member_use
                    value: runPrefs.model,
                    isExpanded: true,
                    decoration: const InputDecoration(labelText: 'Model'),
                    items: runPrefs.modelsForSelected
                        .map(
                          (m) => DropdownMenuItem(
                            value: m,
                            child: Text(m, overflow: TextOverflow.ellipsis),
                          ),
                        )
                        .toList(),
                    onChanged: runPrefs.selectedProvider
                                ?.supportsModelOverride ==
                            false
                        ? null
                        : (v) =>
                            ref.read(runPreferencesProvider.notifier).setModel(v),
                  ),
                  const SizedBox(height: 10),
                  Align(
                    alignment: Alignment.centerLeft,
                    child: TextButton.icon(
                      onPressed: runPrefs.loading
                          ? null
                          : () => ref
                              .read(runPreferencesProvider.notifier)
                              .refresh(),
                      icon: const Icon(Icons.refresh, size: 18),
                      label: const Text('Refresh from Worker'),
                    ),
                  ),
                ],
                if (runPrefs.error != null && runPrefs.providers.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  ConsoleBanner(message: runPrefs.error!),
                ],
                const SizedBox(height: 32),
                Divider(color: theme.dividerTheme.color),
                const SizedBox(height: 24),
                const ConsoleSectionLabel('Session'),
                Text(
                  'Log out removes the operator token from secure storage and clears local caches. There is no offline Start Run queue.',
                  style: theme.textTheme.bodySmall,
                ),
                const SizedBox(height: 14),
                Align(
                  alignment: Alignment.centerLeft,
                  child: OutlinedButton.icon(
                    onPressed: _logout,
                    icon: const Icon(Icons.logout, size: 18),
                    label: const Text('Log out'),
                    style: OutlinedButton.styleFrom(
                      foregroundColor: theme.colorScheme.error,
                    ),
                  ),
                ),
              ],
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
    final tone = switch (result.kind) {
      ConnectivityKind.ok => StatusPillTone.ok,
      ConnectivityKind.unreachable => StatusPillTone.danger,
      ConnectivityKind.authFailure => StatusPillTone.attention,
      ConnectivityKind.unexpected => StatusPillTone.neutral,
    };
    final title = switch (result.kind) {
      ConnectivityKind.ok => 'Connected',
      ConnectivityKind.unreachable => 'Worker unreachable',
      ConnectivityKind.authFailure => 'Authentication failed',
      ConnectivityKind.unexpected => 'Unexpected response',
    };

    return ConsoleBanner(
      message: '$title — ${result.message}',
      tone: tone,
      icon: switch (result.kind) {
        ConnectivityKind.ok => Icons.check_circle_outline,
        ConnectivityKind.unreachable => Icons.cloud_off_outlined,
        ConnectivityKind.authFailure => Icons.lock_outline,
        ConnectivityKind.unexpected => Icons.warning_amber_outlined,
      },
    );
  }
}
