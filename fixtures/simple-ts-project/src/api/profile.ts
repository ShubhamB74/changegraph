import { createSession, isExpired } from "../auth/session";

export function loadProfile(userId: string) {
  const session = createSession(userId, 60_000);
  return { userId, active: !isExpired(session) };
}
