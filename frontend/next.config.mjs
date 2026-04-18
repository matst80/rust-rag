/** @type {import('next').NextConfig} */
const nextConfig = {
  allowedDevOrigins: ["127.0.0.1", "10.10.11.135"],
  typescript: {
    ignoreBuildErrors: true,
  },
  images: {
    unoptimized: true,
  },
  async rewrites() {
    const target = process.env.RAG_API_URL || "http://10.10.11.135:4001"
    return [
      { source: "/admin/:path*", destination: `${target}/admin/:path*` },
      { source: "/graph/:path*", destination: `${target}/graph/:path*` },
      { source: "/search", destination: `${target}/search` },
      { source: "/store", destination: `${target}/store` },
    ]
  },
}

export default nextConfig
