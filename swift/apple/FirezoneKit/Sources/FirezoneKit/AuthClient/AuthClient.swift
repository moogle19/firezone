//
//  AuthClient.swift
//  (c) 2023 Firezone, Inc.
//  LICENSE: Apache-2.0
//

import AuthenticationServices
import Dependencies
import Foundation
import JWTDecode

enum AuthClientError: Error {
  case invalidCallbackURL(URL?)
  case jwtDecoderFailure(Error)
  case sessionFailure(Error)
}

struct AuthClient: Sendable {
  var signIn: @Sendable (URL) async throws -> Token
}

extension AuthClient: DependencyKey {
  static var liveValue: AuthClient {
    let session = WebAuthenticationSession()
    return AuthClient(
      signIn: { host in
        try await session.signIn(host)
      }
    )
  }
}

extension DependencyValues {
  var auth: AuthClient {
    get { self[AuthClient.self] }
    set { self[AuthClient.self] = newValue }
  }
}

private final class WebAuthenticationSession: NSObject,
  ASWebAuthenticationPresentationContextProviding
{
  @MainActor
  func signIn(_ host: URL) async throws -> Token {
    try await withCheckedThrowingContinuation { continuation in
      let callbackURLScheme = "firezone-fd0020211111"
      let session = ASWebAuthenticationSession(
        url: host.appendingPathComponent("auth")
          .appendingQueryItem(URLQueryItem(name: "dest", value: "\(callbackURLScheme)://auth")),
        callbackURLScheme: callbackURLScheme
      ) { callbackURL, error in
        if let error {
          continuation.resume(throwing: AuthClientError.sessionFailure(error))
          return
        }

        guard let callbackURL else {
          continuation.resume(throwing: AuthClientError.invalidCallbackURL(callbackURL))
          return
        }

        guard
          let jwt = URLComponents(url: callbackURL, resolvingAgainstBaseURL: false)?
          .queryItems?
          .first(where: { $0.name == "client_auth_token" })?
          .value
        else {
          continuation.resume(throwing: AuthClientError.invalidCallbackURL(callbackURL))
          return
        }

        do {
          let token = try Token(portalURL: host, tokenString: jwt)
          continuation.resume(returning: token)
        } catch {
          continuation.resume(throwing: AuthClientError.jwtDecoderFailure(error))
        }
      }

      session.presentationContextProvider = self
      session.prefersEphemeralWebBrowserSession = true

      session.start()
    }
  }

  func presentationAnchor(for _: ASWebAuthenticationSession) -> ASPresentationAnchor {
    ASPresentationAnchor()
  }
}

extension URL {
  func appendingQueryItem(_ queryItem: URLQueryItem) -> URL {
    guard var components = URLComponents(url: self, resolvingAgainstBaseURL: false) else {
      return self
    }

    if components.queryItems == nil {
      components.queryItems = []
    }

    components.queryItems!.append(queryItem)
    return components.url ?? self
  }
}
