/** @type {import('next').NextConfig} */
const nextConfig = {
  // The viewer is a static page that reads a JSON report in the browser: no server needed.
  output: "export",
  reactStrictMode: true,
};
export default nextConfig;
