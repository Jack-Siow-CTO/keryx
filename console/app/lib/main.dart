import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'app.dart';
import 'core/credentials_store.dart';
import 'core/device_lock.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(
    ProviderScope(
      overrides: [
        credentialsStoreProvider.overrideWithValue(SecureCredentialsStore()),
        deviceLockProvider.overrideWithValue(LocalAuthDeviceLock()),
      ],
      child: const KeryxConsoleApp(),
    ),
  );
}
