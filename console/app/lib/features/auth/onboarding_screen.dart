import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../widgets/console_chrome.dart';
import 'auth_controller.dart';

/// First-run / logged-out: Worker base URL + operator token.
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
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Connect failed: $e')),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final auth = ref.watch(authControllerProvider);
    final theme = Theme.of(context);
    final size = MediaQuery.sizeOf(context);
    final wide = size.width >= 900;

    return Scaffold(
      body: DecoratedBox(
        decoration: BoxDecoration(
          color: theme.colorScheme.surface,
          gradient: LinearGradient(
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
            colors: [
              theme.colorScheme.surface,
              theme.colorScheme.surfaceContainerLow,
            ],
          ),
        ),
        child: Center(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(24),
            child: ConstrainedBox(
              constraints: BoxConstraints(maxWidth: wide ? 880 : 440),
              child: wide
                  ? Row(
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: [
                        Expanded(child: _BrandPanel(theme: theme)),
                        const SizedBox(width: 40),
                        Expanded(child: _form(theme, auth)),
                      ],
                    )
                  : Column(
                      children: [
                        _BrandPanel(theme: theme, compact: true),
                        const SizedBox(height: 28),
                        _form(theme, auth),
                      ],
                    ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _form(ThemeData theme, AuthState auth) {
    return Material(
      color: theme.colorScheme.surfaceContainerLowest,
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(16),
        side: BorderSide(color: theme.colorScheme.outlineVariant),
      ),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(24, 28, 24, 24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text('Connect to Worker', style: theme.textTheme.titleMedium),
            const SizedBox(height: 6),
            Text(
              'Principal access over your Tailnet. Token stays in secure storage.',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 22),
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
            const SizedBox(height: 14),
            TextField(
              controller: _tokenController,
              decoration: InputDecoration(
                labelText: 'Operator token',
                hintText: 'Paste token (not “Bearer …”)',
                suffixIcon: IconButton(
                  icon: Icon(
                    _obscure ? Icons.visibility_outlined : Icons.visibility_off_outlined,
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
              subtitle: Text(
                'Biometric or device credential on open',
                style: theme.textTheme.bodySmall,
              ),
              value: _biometric,
              onChanged: _busy ? null : (v) => setState(() => _biometric = v),
            ),
            if (auth.lastConnectivity != null && !auth.lastConnectivity!.isOk) ...[
              const SizedBox(height: 8),
              ConsoleBanner(message: auth.lastConnectivity!.message),
            ],
            if (auth.errorMessage != null) ...[
              const SizedBox(height: 8),
              ConsoleBanner(message: auth.errorMessage!),
            ],
            const SizedBox(height: 18),
            FilledButton(
              onPressed: _busy ? null : _submit,
              child: _busy
                  ? const SizedBox(
                      height: 18,
                      width: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Text('Connect'),
            ),
          ],
        ),
      ),
    );
  }
}

class _BrandPanel extends StatelessWidget {
  const _BrandPanel({required this.theme, this.compact = false});

  final ThemeData theme;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment:
          compact ? CrossAxisAlignment.center : CrossAxisAlignment.start,
      children: [
        Text(
          'Keryx',
          style: theme.textTheme.headlineMedium?.copyWith(
            fontWeight: FontWeight.w700,
            letterSpacing: -0.8,
          ),
        ),
        Text(
          'Console',
          style: theme.textTheme.headlineSmall?.copyWith(
            fontWeight: FontWeight.w500,
            color: theme.colorScheme.primary,
            letterSpacing: -0.4,
          ),
        ),
        SizedBox(height: compact ? 12 : 20),
        Text(
          'Primary operator surface for Sessions, Runs, Approvals, Memory, and Schedules. Worker remains system of record.',
          textAlign: compact ? TextAlign.center : TextAlign.start,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
            height: 1.5,
          ),
        ),
        if (!compact) ...[
          const SizedBox(height: 28),
          const Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              StatusPill(label: 'Chat list home', tone: StatusPillTone.neutral),
              StatusPill(label: 'Thin Principal client', tone: StatusPillTone.neutral),
              StatusPill(label: 'Needs you', tone: StatusPillTone.attention),
            ],
          ),
        ],
      ],
    );
  }
}
