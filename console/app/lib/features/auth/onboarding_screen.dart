import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'auth_controller.dart';

/// First-run / logged-out: enter Worker base URL + operator token.
class OnboardingScreen extends ConsumerStatefulWidget {
  const OnboardingScreen({super.key});

  @override
  ConsumerState<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends ConsumerState<OnboardingScreen> {
  final _urlController = TextEditingController(text: 'http://127.0.0.1:8787');
  final _tokenController = TextEditingController();
  bool _biometric = false;
  bool _obscure = true;
  bool _busy = false;

  @override
  void dispose() {
    _urlController.dispose();
    _tokenController.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    setState(() => _busy = true);
    try {
      await ref.read(authControllerProvider.notifier).saveConnection(
            baseUrl: _urlController.text,
            operatorToken: _tokenController.text,
            biometricLockEnabled: _biometric,
          );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final auth = ref.watch(authControllerProvider);
    final theme = Theme.of(context);

    return Scaffold(
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 440),
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  'Keryx Console',
                  style: theme.textTheme.headlineMedium?.copyWith(
                    fontWeight: FontWeight.w600,
                  ),
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 8),
                Text(
                  'Connect as Principal to your Worker control plane. '
                  'The operator token is stored in OS secure storage only.',
                  style: theme.textTheme.bodyMedium?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 32),
                TextField(
                  controller: _urlController,
                  decoration: const InputDecoration(
                    labelText: 'Worker base URL',
                    hintText: 'https://keryx.tailnet.ts.net',
                  ),
                  keyboardType: TextInputType.url,
                  autocorrect: false,
                  enabled: !_busy,
                ),
                const SizedBox(height: 16),
                TextField(
                  controller: _tokenController,
                  decoration: InputDecoration(
                    labelText: 'Operator token',
                    hintText: 'Bearer token (never logged)',
                    suffixIcon: IconButton(
                      icon: Icon(
                        _obscure ? Icons.visibility : Icons.visibility_off,
                      ),
                      onPressed: () => setState(() => _obscure = !_obscure),
                    ),
                  ),
                  obscureText: _obscure,
                  autocorrect: false,
                  enableSuggestions: false,
                  enabled: !_busy,
                  onSubmitted: (_) => _submit(),
                ),
                const SizedBox(height: 8),
                SwitchListTile(
                  contentPadding: EdgeInsets.zero,
                  title: const Text('Require device unlock'),
                  subtitle: const Text(
                    'Biometric or device credential when opening Console',
                  ),
                  value: _biometric,
                  onChanged: _busy
                      ? null
                      : (v) => setState(() => _biometric = v),
                ),
                if (auth.errorMessage != null) ...[
                  const SizedBox(height: 8),
                  Text(
                    auth.errorMessage!,
                    style: TextStyle(color: theme.colorScheme.error),
                  ),
                ],
                const SizedBox(height: 24),
                FilledButton(
                  onPressed: _busy ? null : _submit,
                  child: _busy
                      ? const SizedBox(
                          height: 20,
                          width: 20,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Text('Connect'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
