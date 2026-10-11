import { loadProfile } from "../src/api/profile";

test("profile is active for a fresh session", () => {
  expect(loadProfile("u1").active).toBe(true);
});
