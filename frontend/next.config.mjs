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
    return [
      {
        source: '/api/:path((?!auth/).*)',
        destination: (process.env.RAG_API_URL || 'http://localhost:4001') + '/api/:path*',
      },
    ]
  },
}

export default nextConfig
