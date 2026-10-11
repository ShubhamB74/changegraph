import { createSession, isExpired } from "../src/auth/session";

test("a fresh session is not expired", () => {
  expect(isExpired(createSession("u1", 1000))).toBe(false);
});
