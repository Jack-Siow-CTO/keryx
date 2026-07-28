/// Errors raised by [KeryxApiClient] when talking to the control plane.
sealed class KeryxApiException implements Exception {
  const KeryxApiException(this.message);

  final String message;

  @override
  String toString() => '$runtimeType: $message';
}

/// TCP/TLS/DNS failure or non-HTTP transport problem (Worker unreachable).
final class KeryxUnreachableException extends KeryxApiException {
  const KeryxUnreachableException(super.message, {this.cause});

  final Object? cause;
}

/// HTTP 401 / missing-or-invalid operator token. Fail closed.
final class KeryxAuthException extends KeryxApiException {
  const KeryxAuthException(super.message, {this.statusCode = 401});

  final int statusCode;
}

/// Any other non-success HTTP status from the control plane.
final class KeryxHttpException extends KeryxApiException {
  const KeryxHttpException(
    super.message, {
    required this.statusCode,
    this.body,
  });

  final int statusCode;
  final String? body;
}

/// Local programming / decode error (malformed JSON, etc.).
final class KeryxClientException extends KeryxApiException {
  const KeryxClientException(super.message, {this.cause});

  final Object? cause;
}
