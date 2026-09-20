FROM node:22-slim AS deps

RUN corepack enable

WORKDIR /app

COPY shared/ui ./shared/ui
COPY shared/llm ./shared/llm
COPY frontend/package.json frontend/pnpm-lock.yaml ./frontend/

WORKDIR /app/frontend

RUN pnpm install --frozen-lockfile --ignore-scripts

FROM node:22-slim AS builder

RUN corepack enable

WORKDIR /app

COPY --from=deps /app/frontend/node_modules ./frontend/node_modules
COPY shared/ui ./shared/ui
COPY shared/llm ./shared/llm
COPY frontend ./frontend

WORKDIR /app/frontend

# Immutable deploy ID baked into the client bundle; must exactly match the
# source-map upload release (scripts/upload-sourcemaps.sh).
ARG NEXT_PUBLIC_APP_RELEASE=dev

ENV NEXT_TELEMETRY_DISABLED=1 \
	CI=true \
	NEXT_PUBLIC_APP_RELEASE=${NEXT_PUBLIC_APP_RELEASE}

RUN pnpm build

# Build-stage snapshot used to extract browser source maps from the exact
# deployed build: docker build --target sourcemaps, then docker cp the
# /app/frontend/.next/static tree out and upload the *.js.map files.
FROM builder AS sourcemaps

FROM node:22-slim AS runner

RUN groupadd --system --gid 1001 nodejs \
	&& useradd --system --uid 1001 --gid 1001 --no-create-home --shell /usr/sbin/nologin nextjs

WORKDIR /app

ENV NODE_ENV=production \
	NEXT_TELEMETRY_DISABLED=1 \
	PORT=3000 \
	HOSTNAME=0.0.0.0

COPY --from=builder /app/frontend/public ./frontend/public
COPY --from=builder --chown=nextjs:nodejs /app/frontend/.next/standalone ./
COPY --from=builder --chown=nextjs:nodejs /app/frontend/.next/static ./frontend/.next/static

# Maps were uploaded from the sourcemaps stage; don't serve them publicly.
RUN find /app/frontend/.next -name "*.map" -delete

USER nextjs

EXPOSE 3000

CMD ["node", "frontend/server.js"]
