import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Last-fetched presentation cache only (ADR 0019). Not a write replica.
///
/// Ticket #38: empty placeholders; later tickets populate Session list, etc.
final class SessionCache {
  final List<Object> sessions = [];
  final List<Object> inboxItems = [];

  void clear() {
    sessions.clear();
    inboxItems.clear();
  }
}

final sessionCacheProvider = Provider<SessionCache>((ref) => SessionCache());
