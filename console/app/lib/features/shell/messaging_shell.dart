import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../widgets/console_chrome.dart';
import '../artifacts/artifact_viewer.dart';
import '../auth/auth_controller.dart';
import '../inbox/inbox_screen.dart';
import '../sessions/session_detail.dart';
import '../sessions/session_info.dart';
import '../sessions/sessions_controller.dart';
import '../sessions/sessions_list.dart';
import 'profile_hub.dart';

/// What the detail / third pane is showing (messaging shell, ADR 0031).
enum MessagingDetail {
  /// No Session open — empty prompt.
  none,

  /// Needs you Inbox projection (not a Session thread).
  needsYou,

  /// Open Session thread.
  session,

  /// Per-Session title / Policy / Workspace (wide third pane or push).
  sessionInfo,

  /// Artifact contextual pane (wide) or push (narrow).
  artifact,
}

/// Messenger master–detail shell (ADRs 0031–0032). Replaces dual-rail home.
///
/// Wide (≥1100): chat list | thread; optional third pane for Session info / Artifact.
/// Medium/narrow: list first; thread and hub are pushed full-screen detail.
class MessagingShell extends ConsumerStatefulWidget {
  const MessagingShell({super.key});

  static const wideBreakpoint = 1100.0;
  static const mediumBreakpoint = 720.0;

  @override
  ConsumerState<MessagingShell> createState() => _MessagingShellState();
}

class _MessagingShellState extends ConsumerState<MessagingShell> {
  MessagingDetail _detail = MessagingDetail.none;
  String? _artifactId;

  @override
  Widget build(BuildContext context) {
    final width = MediaQuery.sizeOf(context).width;
    if (width >= MessagingShell.wideBreakpoint) {
      return _wideLayout(context);
    }
    return _stackedLayout(context);
  }

  // ── Wide: list | thread | optional third ─────────────────────────────

  Widget _wideLayout(BuildContext context) {
    final theme = Theme.of(context);
    final showThird = _detail == MessagingDetail.sessionInfo ||
        _detail == MessagingDetail.artifact;

    return Scaffold(
      appBar: _appBar(context, title: 'Keryx'),
      body: Row(
        children: [
          _listPanel(
            width: 320,
            child: ChatListPane(
              needsYouSelected: _detail == MessagingDetail.needsYou,
              onSelectNeedsYou: () => setState(() {
                _detail = MessagingDetail.needsYou;
                _artifactId = null;
              }),
              onSelectSession: (_) => setState(() {
                _detail = MessagingDetail.session;
                _artifactId = null;
              }),
              onNewChat: (_) => setState(() {
                _detail = MessagingDetail.session;
                _artifactId = null;
              }),
            ),
          ),
          Expanded(child: _detailBody(context)),
          if (showThird)
            _listPanel(
              width: 360,
              trailingBorder: false,
              leadingBorder: true,
              child: _thirdPane(context),
            ),
        ],
      ),
      backgroundColor: theme.colorScheme.surface,
    );
  }

  // ── Medium / narrow: list home, push detail ──────────────────────────

  Widget _stackedLayout(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: _appBar(context, title: 'Chats'),
      body: ChatListPane(
        needsYouSelected: false,
        onSelectNeedsYou: () => _push(
          context,
          title: 'Needs you',
          body: NeedsYouPane(
            onOpenSession: (id) {
              Navigator.of(context).pop();
              _pushSession(context, id);
            },
          ),
        ),
        onSelectSession: (id) => _pushSession(context, id),
        onNewChat: (id) => _pushSession(context, id),
      ),
      backgroundColor: theme.colorScheme.surface,
    );
  }

  void _pushSession(BuildContext context, String sessionId) {
    setState(() {
      _detail = MessagingDetail.session;
      _artifactId = null;
    });
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => SessionThreadPage(
          onOpenSessionInfo: () {
            Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => Scaffold(
                  appBar: AppBar(title: const Text('Session info')),
                  body: SessionInfoPane(
                    onClose: () => Navigator.of(context).pop(),
                  ),
                ),
              ),
            );
          },
          onOpenArtifact: (artifactId) {
            Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => Scaffold(
                  appBar: AppBar(title: const Text('Artifact')),
                  body: ArtifactViewerPage(artifactId: artifactId),
                ),
              ),
            );
          },
        ),
      ),
    );
  }

  void _push(
    BuildContext context, {
    required String title,
    required Widget body,
  }) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => Scaffold(
          appBar: AppBar(title: Text(title)),
          body: body,
        ),
      ),
    );
  }

  Widget _detailBody(BuildContext context) {
    switch (_detail) {
      case MessagingDetail.none:
        return const ConsoleEmptyState(
          icon: Icons.chat_bubble_outline,
          title: 'Select a chat',
          body:
              'Open a Session from the list, or start a New chat. Needs you surfaces Approvals and failed Runs.',
        );
      case MessagingDetail.needsYou:
        return NeedsYouPane(
          onOpenSession: (_) => setState(() {
            _detail = MessagingDetail.session;
            _artifactId = null;
          }),
        );
      case MessagingDetail.session:
      case MessagingDetail.sessionInfo:
      case MessagingDetail.artifact:
        return SessionDetailPane(
          onOpenSessionInfo: () => setState(() {
            _detail = MessagingDetail.sessionInfo;
            _artifactId = null;
          }),
          onOpenArtifact: (id) => setState(() {
            _detail = MessagingDetail.artifact;
            _artifactId = id;
          }),
        );
    }
  }

  Widget _thirdPane(BuildContext context) {
    if (_detail == MessagingDetail.artifact && _artifactId != null) {
      return ArtifactViewerPane(
        artifactId: _artifactId!,
        onClose: () => setState(() {
          _detail = MessagingDetail.session;
          _artifactId = null;
        }),
      );
    }
    return SessionInfoPane(
      onClose: () => setState(() => _detail = MessagingDetail.session),
    );
  }

  PreferredSizeWidget _appBar(BuildContext context, {required String title}) {
    final auth = ref.watch(authControllerProvider);
    final conn = auth.lastConnectivity;

    return AppBar(
      title: Row(
        children: [
          Text(title),
          if (conn != null) ...[
            const SizedBox(width: 12),
            StatusPill(
              label: conn.isOk ? 'Worker OK' : _connShort(conn),
              icon: conn.isOk
                  ? Icons.cloud_done_outlined
                  : Icons.cloud_off_outlined,
              tone: conn.isOk ? StatusPillTone.ok : StatusPillTone.danger,
            ),
          ],
        ],
      ),
      actions: [
        IconButton(
          tooltip: 'Profile and tools',
          icon: const Icon(Icons.account_circle_outlined, size: 22),
          onPressed: () => _openHub(context),
        ),
        const SizedBox(width: 4),
      ],
    );
  }

  void _openHub(BuildContext context) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(builder: (_) => const ProfileHubPage()),
    );
  }

  String _connShort(ConnectivityResult conn) {
    return switch (conn.kind) {
      ConnectivityKind.unreachable => 'Unreachable',
      ConnectivityKind.authFailure => 'Auth failed',
      ConnectivityKind.unexpected => 'Unexpected',
      ConnectivityKind.ok => 'OK',
    };
  }

  Widget _listPanel({
    required Widget child,
    required double width,
    bool trailingBorder = true,
    bool leadingBorder = false,
  }) {
    final theme = Theme.of(context);
    return SizedBox(
      width: width,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: theme.colorScheme.surfaceContainerLow,
          border: Border(
            right: trailingBorder
                ? BorderSide(
                    color: theme.dividerTheme.color ?? theme.dividerColor,
                  )
                : BorderSide.none,
            left: leadingBorder
                ? BorderSide(
                    color: theme.dividerTheme.color ?? theme.dividerColor,
                  )
                : BorderSide.none,
          ),
        ),
        child: child,
      ),
    );
  }
}

/// Full-screen Session thread for stacked (medium/narrow) navigation.
class SessionThreadPage extends ConsumerWidget {
  const SessionThreadPage({
    super.key,
    this.onOpenSessionInfo,
    this.onOpenArtifact,
  });

  final VoidCallback? onOpenSessionInfo;
  final ValueChanged<String>? onOpenArtifact;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final session = ref.watch(sessionsControllerProvider).selected;
    final pending = session?.pendingApprovalCount ?? 0;
    final active = session?.activeRootRun?.status == 'active';
    return Scaffold(
      appBar: AppBar(
        title: Text(session?.title ?? 'Chat'),
        actions: [
          if (active)
            const Padding(
              padding: EdgeInsets.only(right: 4),
              child: Center(
                child: StatusPill(
                  label: 'Active',
                  tone: StatusPillTone.active,
                ),
              ),
            ),
          if (pending > 0)
            Padding(
              padding: const EdgeInsets.only(right: 4),
              child: Center(child: AttentionBadge(count: pending)),
            ),
          if (onOpenSessionInfo != null)
            IconButton(
              tooltip: 'Session info',
              icon: const Icon(Icons.info_outline, size: 20),
              onPressed: onOpenSessionInfo,
            ),
        ],
      ),
      body: SessionDetailPane(
        showHeader: false,
        onOpenSessionInfo: onOpenSessionInfo,
        onOpenArtifact: onOpenArtifact,
      ),
    );
  }
}
