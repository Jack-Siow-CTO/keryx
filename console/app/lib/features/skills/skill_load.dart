import 'package:keryx_api/keryx_api.dart';

/// Pure helpers for Skill load indicators (ADR 0030 / #82).
///
/// Worker writes `skill_load name={package}` in tool summary and embeds the
/// package name in live tool event labels. Console stays read-mostly: it only
/// surfaces list/load transparency, never writes packages.

/// True when a tool name (durable or live) is skill_load.
///
/// Live finished events use `skill_load: skill_load name=…` (colon form).
bool isSkillLoadTool(String name) {
  final n = name.trim().toLowerCase();
  return n == 'skill_load' ||
      n.startsWith('skill_load ') ||
      n.startsWith('skill_load:') ||
      n.startsWith('skill_load(');
}

/// Extract package name from skill_load tool rows or live event labels.
///
/// Accepts:
/// - durable summary: `skill_load name=daily-note`
/// - live started: `skill_load (name=daily-note)`
/// - live finished: `skill_load: skill_load name=daily-note`
String? skillNameFromLoadSignal({
  String? toolName,
  String? summary,
  String? eventName,
}) {
  for (final raw in [summary, eventName, toolName]) {
    if (raw == null || raw.isEmpty) continue;
    final fromNamed = _nameAfterKey(raw);
    if (fromNamed != null) return fromNamed;
  }
  return null;
}

/// Ordered unique Skill package names loaded in Transcript tool rows.
List<String> loadedSkillsFromMessages(Iterable<TranscriptMessage> messages) {
  final seen = <String>{};
  final out = <String>[];
  for (final m in messages) {
    final tool = m.tool;
    if (tool == null) continue;
    if (!isSkillLoadTool(tool.name)) continue;
    if (tool.status == 'error') continue;
    final name = skillNameFromLoadSignal(
      toolName: tool.name,
      summary: tool.summary,
    );
    if (name == null || name.isEmpty) continue;
    if (seen.add(name)) out.add(name);
  }
  return out;
}

/// Activity title for skill_load (thread + live strip).
String skillLoadActivityTitle(String? skillName) {
  if (skillName == null || skillName.isEmpty) return 'Skill load';
  return 'Skill · $skillName';
}

/// Status-strip label for a live skill_load row.
String skillLoadStripLabel(String? skillName, String status) {
  final title = skillLoadActivityTitle(skillName);
  return '$title · $status';
}

final _nameKey = RegExp(
  r'name\s*=\s*([A-Za-z0-9][A-Za-z0-9._-]{0,63})',
  caseSensitive: false,
);

String? _nameAfterKey(String raw) {
  final m = _nameKey.firstMatch(raw);
  if (m == null) return null;
  final name = m.group(1);
  if (name == null || name.isEmpty) return null;
  return name;
}
