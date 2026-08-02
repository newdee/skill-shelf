# Build the frontend and serve it via nginx, proxying /api to the server.
FROM node:24-slim AS build
WORKDIR /app
# Pin the package manager. Without a pin, corepack resolves pnpm to `latest` —
# a moving target that silently changes the build. pnpm 11 requires Node
# >=22.13 and fails here with ERR_UNKNOWN_BUILTIN_MODULE; a future major will
# raise the floor again. Stay on 10.x: it writes the same lockfileVersion 9.0
# that pnpm-lock.yaml is committed with.
RUN corepack enable && corepack prepare pnpm@10.34.5 --activate
COPY package.json pnpm-lock.yaml* ./
# No `|| pnpm install` fallback: that turns a stale lockfile into a silently
# different dependency tree. Fail loudly instead and regenerate the lockfile.
RUN pnpm install --frozen-lockfile
COPY . .
# Same build points at any backend at runtime (Settings page); default to /api.
ENV VITE_API_BASE=/api
RUN pnpm build

FROM nginx:1-alpine
COPY --from=build /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
