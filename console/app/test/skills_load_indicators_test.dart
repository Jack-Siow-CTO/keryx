import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:keryx_console/features/sessions/session_run_controller.dart';
import 'package:keryx_console/features/sessions/transcript_pane.dart';
import 'package:keryx_console/features/skills/skill_load.dart';
import 'package:keryx_console/features/skills/skills_screen.dart';
import 'package:keryx_console/theme/keryx_theme.dart';
import 'package:keryx_console/widgets/console_chrome.dart';

RunEvent _event({
  required String type,
  Map<String, dynamic>? payload,
  int seq = 1,
}) {
  final kind = <String, dynamic>{'type': type};
  if (payload != null) kind['payload'] = payload;
  return RunEvent(
    runId: 'r1',
    seq: seq,
    type: type,
    raw: {
      'run_id': 'r1',
      'seq': seq,
      'kind': kind,
    },
    payload: payload ?? const {},
  );
}

void main() {
  group('skill_load pure helpers', () {
    test('parses durable skill_load summary', () {
      expect(
        skillNameFromLoadSignal(summary: 'skill_load name=daily-note'),
        'daily-note',
      );
      expect(isSkillLoadTool('skill_load'), isTrue);
      expect(isSkillLoadTool('workspace_read'), isFalse);
    });

    test('parses live tool event labels from Worker', () {
      expect(
        skillNameFromLoadSignal(eventName: 'skill_load (name=daily-note)'),
        'daily-note',
      );
      expect(
        skillNameFromLoadSignal(
          eventName: 'skill_load: skill_load name=triage',
        ),
        'triage',
      );
      expect(isSkillLoadTool('skill_load (name=x)'), isTrue);
    });

    test('loadedSkillsFromMessages collects ok skill_load rows only', () {
      const messages = [
        TranscriptMessage(
          id: 'm1',
          role: 'tool',
          content: 'x',
          createdAt: 1,
          tool: ToolCompact(
            name: 'skill_load',
            status: 'ok',
            summary: 'skill_load name=daily-note',
          ),
        ),
        TranscriptMessage(
          id: 'm2',
          role: 'tool',
          content: 'x',
          createdAt: 2,
          tool: ToolCompact(
            name: 'workspace_read',
            status: 'ok',
            summary: 'read',
          ),
        ),
        TranscriptMessage(
          id: 'm3',
          role: 'tool',
          content: 'x',
          createdAt: 3,
          tool: ToolCompact(
            name: 'skill_load',
            status: 'error',
            summary: 'skill not found: missing',
          ),
        ),
        TranscriptMessage(
          id: 'm4',
          role: 'tool',
          content: 'x',
          createdAt: 4,
          tool: ToolCompact(
            name: 'skill_load',
            status: 'ok',
            summary: 'skill_load name=daily-note',
          ),
        ),
        TranscriptMessage(
          id: 'm5',
          role: 'tool',
          content: 'x',
          createdAt: 5,
          tool: ToolCompact(
            name: 'skill_load',
            status: 'ok',
            summary: 'skill_load name=triage',
          ),
        ),
      ];
      expect(
        loadedSkillsFromMessages(messages),
        ['daily-note', 'triage'],
      );
    });

    test('activity title and strip label', () {
      expect(skillLoadActivityTitle('daily-note'), 'Skill · daily-note');
      expect(skillLoadActivityTitle(null), 'Skill load');
      expect(
        skillLoadStripLabel('daily-note', 'loaded'),
        'Skill · daily-note · loaded',
      );
    });
  });

  group('live activity skill_load', () {
    test('skill_load start/finish collapses with Skill title', () {
      var items = const <LiveActivityItem>[];
      items = applyLiveActivity(
        items,
        _event(
          type: 'tool_started',
          payload: {'name': 'skill_load (name=daily-note)'},
        ),
      )!;
      expect(items, hasLength(1));
      expect(items.single.id, 'tool:skill_load:daily-note');
      expect(items.single.title, 'Skill · daily-note');
      expect(items.single.status, 'running');
      expect(items.single.stripLabel, 'Skill · daily-note · running');

      items = applyLiveActivity(
        items,
        _event(
          type: 'tool_finished',
          seq: 2,
          payload: {'name': 'skill_load: skill_load name=daily-note'},
        ),
      )!;
      expect(items, hasLength(1), reason: 'finish must upsert same skill row');
      expect(items.single.status, 'loaded');
      expect(items.single.summary, contains('daily-note'));
    });
  });

  group('Skills list Available indicator', () {
    testWidgets('Available pill marks packages ready for skill_load',
        (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: KeryxTheme.light(),
          home: Scaffold(
            body: ConsoleListRow(
              leading: const Icon(Icons.extension_outlined),
              title: 'daily-note',
              subtitle: 'Package ready for skill_load',
              trailing: const StatusPill(
                icon: Icons.check_circle_outline,
                label: 'Available',
                tone: StatusPillTone.ok,
              ),
              onTap: () {},
            ),
          ),
        ),
      );
      expect(find.text('daily-note'), findsOneWidget);
      expect(find.text('Available'), findsOneWidget);
      expect(find.textContaining('skill_load'), findsOneWidget);
    });

    testWidgets('Skill detail shows Available for load chip', (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: KeryxTheme.light(),
          home: const SkillDetailPage(
            detail: SkillDetail(
              name: 'daily-note',
              content: '# daily-note\n\nCapture notes.\n',
            ),
          ),
        ),
      );
      expect(find.text('daily-note'), findsWidgets);
      expect(find.text('Available for load'), findsOneWidget);
      expect(find.textContaining('Capture notes'), findsOneWidget);
    });
  });

  group('Session load indicators', () {
    testWidgets('LoadedSkillsStrip shows package chips', (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: KeryxTheme.light(),
          home: const Scaffold(
            body: LoadedSkillsStrip(
              skillNames: ['daily-note', 'triage'],
            ),
          ),
        ),
      );
      expect(find.text('Skills loaded'), findsOneWidget);
      expect(find.text('daily-note'), findsOneWidget);
      expect(find.text('triage'), findsOneWidget);
    });

    testWidgets('skill_load ActivityBlock uses Skill title and loaded status',
        (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: KeryxTheme.light(),
          home: Scaffold(
            body: ActivityBlock(
              blockId: 't1',
              title: skillLoadActivityTitle('daily-note'),
              status: 'loaded',
              summary: 'skill_load name=daily-note',
              looksLikeChild: false,
              looksLikeSkill: true,
              expanded: true,
              onToggle: () {},
            ),
          ),
        ),
      );
      expect(find.text('Skill · daily-note'), findsOneWidget);
      expect(find.text('loaded'), findsOneWidget);
      expect(
        find.textContaining('Skill package loaded into this Run'),
        findsOneWidget,
      );
    });
  });
}
