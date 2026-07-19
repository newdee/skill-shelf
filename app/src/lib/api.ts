import axios from "axios";
import { getApiBase } from "./config";

// Single HTTP client. The base URL is read per-request so changing it in
// Settings takes effect immediately, no rebuild.
const TOKEN_KEY = "skillshelf.token";

const http = axios.create();
http.interceptors.request.use((cfg) => {
  cfg.baseURL = getApiBase();
  const token = localStorage.getItem(TOKEN_KEY);
  if (token) cfg.headers.Authorization = `Bearer ${token}`;
  return cfg;
});

export type Kind = "skill" | "prompt";

export interface Skill {
  id: string;
  name: string;
  description: string;
  kind: Kind;
  license?: string;
  compatibility?: string;
  metadata?: Record<string, string>;
  allowed_tools?: string;
  created_at: number;
}

export interface SkillMeta {
  name: string;
  description: string;
  license?: string;
  compatibility?: string;
  metadata?: Record<string, string>;
  allowed_tools?: string;
}

export interface ValidateResult {
  ok: boolean;
  error?: string;
  meta?: SkillMeta;
  warnings?: string[];
}

export interface Branch {
  skill_id: string;
  name: string;
  head: string | null;
}

export interface Commit {
  id: string;
  tree: string;
  parent: string | null;
  author: string;
  message: string;
  timestamp: number;
}

export interface TreeEntry {
  path: string;
  hash: string;
  size: number;
}

export interface FileDiff {
  path: string;
  status: "added" | "removed" | "modified" | "unchanged";
  diff: string | null;
}

export interface RouteResult {
  skill_id: string;
  name: string;
  description: string;
  score: number;
}

export interface Feedback {
  id: string;
  skill_id: string;
  commit_id?: string;
  source: string;
  rating: number;
  content: string;
  query?: string;
  status: "open" | "applied" | "dismissed";
  created_at: number;
}

export interface RefineResult {
  commit: Commit;
  diff: FileDiff[];
}

// UTF-8 safe base64 (backend stores file bytes as base64).
export function encodeContent(text: string): string {
  return btoa(String.fromCharCode(...new TextEncoder().encode(text)));
}
export function decodeContent(b64: string): string {
  return new TextDecoder().decode(Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)));
}

export interface AuthResponse {
  token: string;
  username: string;
  role: string;
}

export interface ConfigVar {
  key: string;
  secret: boolean;
  set: boolean;
  is_json: boolean;
  value?: unknown;
  source: string;
}
export interface ConfigView {
  vars: ConfigVar[];
  ai_ready: boolean;
  locked_keys: string[];
}

// ---- config center ----
export interface NamespaceInfo {
  name: string;
  keys: number;
}
export interface NamespaceView {
  namespace: string;
  vars: ConfigVar[];
}
export interface ClientView {
  id: string;
  name: string;
  namespaces: string[];
  created_at: number;
}
export interface NewClient {
  id: string;
  name: string;
  namespaces: string[];
  token: string; // shown once
}

export const api = {
  status: () =>
    http.get<{ status: string; service: string; version: string; auth_enabled: boolean }>("/status").then((r) => r.data),

  signup: (body: { username: string; password: string }) =>
    http.post<AuthResponse>("/user/signup", body).then((r) => r.data),
  signin: (body: { username: string; password: string }) =>
    http.post<AuthResponse>("/user/signin", body).then((r) => r.data),

  getConfig: () => http.get<ConfigView>("/config").then((r) => r.data),
  putConfig: (patch: Record<string, unknown>) =>
    http.put<ConfigView>("/config", patch).then((r) => r.data),

  // config center (admin)
  listNamespaces: () => http.get<NamespaceInfo[]>("/config/namespaces").then((r) => r.data),
  getNamespace: (namespace: string) =>
    http.get<NamespaceView>("/config/namespace", { params: { namespace } }).then((r) => r.data),
  putNamespace: (namespace: string, patch: Record<string, unknown>) =>
    http.put<NamespaceView>("/config/namespace", patch, { params: { namespace } }).then((r) => r.data),
  listClients: () => http.get<ClientView[]>("/config/clients").then((r) => r.data),
  createClient: (body: { name: string; namespaces: string[] }) =>
    http.post<NewClient>("/config/clients", body).then((r) => r.data),
  deleteClient: (id: string) => http.delete(`/config/clients/${id}`).then(() => undefined),

  listSkills: (params?: { q?: string; kind?: string }) =>
    http.get<Skill[]>("/skill", { params }).then((r) => r.data),
  getSkill: (id: string) => http.get<Skill>(`/skill/${id}`).then((r) => r.data),
  createSkill: (body: { name: string; description: string; kind: Kind }) =>
    http.post<Skill>("/skill", body).then((r) => r.data),
  deleteSkill: (id: string) => http.delete(`/skill/${id}`).then(() => undefined),

  listBranches: (id: string) =>
    http.get<Branch[]>(`/skill/${id}/branches`).then((r) => r.data),

  listCommits: (id: string, branch = "main") =>
    http.get<Commit[]>(`/skill/${id}/commits`, { params: { branch } }).then((r) => r.data),
  getTree: (cid: string) =>
    http.get<TreeEntry[]>(`/commit/${cid}/tree`).then((r) => r.data),
  getFile: (cid: string, path: string) =>
    http
      .get<{ path: string; content: string }>(`/commit/${cid}/file`, { params: { path } })
      .then((r) => decodeContent(r.data.content)),

  commit: (
    id: string,
    body: {
      branch?: string;
      author: string;
      message: string;
      files: { path: string; content: string }[]; // content is base64
    },
  ) => http.post<Commit>(`/skill/${id}/commit`, body).then((r) => r.data),

  diff: (a: string, b: string) =>
    http.get<FileDiff[]>("/diff", { params: { a, b } }).then((r) => r.data),

  rollback: (id: string, body: { branch?: string; to_commit: string; author: string }) =>
    http.post<Commit>(`/skill/${id}/rollback`, body).then((r) => r.data),

  route: (body: {
    query: string;
    top_k?: number;
    mode?: "fuzzy" | "exact" | "smart";
    rerank?: boolean;
  }) => http.post<RouteResult[]>("/route", body).then((r) => r.data),

  validate: (files: { path: string; content: string }[]) =>
    http.post<ValidateResult>("/validate", { files }).then((r) => r.data),

  importZip: (file: File) =>
    http
      .post<Skill>("/skill/import", file, { headers: { "Content-Type": "application/zip" } })
      .then((r) => r.data),

  importGithub: (body: { url: string; ref?: string; subpath?: string }) =>
    http
      .post<{ imported: Skill[]; skipped: string[] }>("/skill/import/github", body)
      .then((r) => r.data),

  exportUrl: (id: string) => `${getApiBase()}/skill/${id}/export`,

  listFeedback: (id: string) =>
    http.get<Feedback[]>(`/skill/${id}/feedback`).then((r) => r.data),
  addFeedback: (
    id: string,
    body: { rating: number; content: string; source?: string; query?: string; commit_id?: string },
  ) => http.post<Feedback>(`/skill/${id}/feedback`, body).then((r) => r.data),
  setFeedbackStatus: (id: string, fid: string, status: string) =>
    http.post(`/skill/${id}/feedback/${fid}/status`, { status }).then(() => undefined),

  refine: (id: string) => http.post<RefineResult>(`/skill/${id}/refine`).then((r) => r.data),
  refineMerge: (id: string) => http.post<Commit>(`/skill/${id}/refine/merge`).then((r) => r.data),
};
