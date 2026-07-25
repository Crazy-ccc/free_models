export interface Provider {
  id: number;
  name: string;
  base_url: string;
  created_time?: string;
  last_updated?: string;
}

export interface ProviderCredential {
  id: number;
  provider_id: number;
  name: string;
  api_key: string;
  account: string | null;
  password: string | null;
  priority: number;
  is_active: boolean;
  created_time?: string;
  last_updated?: string;
}

export interface Model {
  id: number;
  name: string;
  timeout: number;
  context_length: number;
  priority: number;
  is_active: boolean;
  created_time?: string;
  last_updated?: string;
}

export interface ProviderModelMap {
  id: number;
  model_id: number;
  provider_id: number;
  provider_model_id: string;
  protocols: string;
  priority: number;
  status: string;
  is_active: boolean;
  created_time?: string;
  last_updated?: string;
}

export interface ApiKey {
  id: number;
  key_value: string;
  name: string;
  is_active: boolean;
  created_time?: string;
  last_updated?: string;
}

export type PageKey = 'overview' | 'providers' | 'models' | 'apiKeys' | 'stats' | 'settings';

export interface ServiceStatus {
  healthy: boolean;
  models: { total: number; active: number; inactive: number };
  providers: { total: number; active: number; inactive: number };
  apiKeys: { total: number; active: number; inactive: number };
  penalties: Array<{ modelName: string; providerName: string; remainingSecs: number }>;
}

export interface TestCredentialResult {
  success: boolean;
  response_time_ms: number;
  model_id: string;
  provider_id: number;
  credential_id: number;
  error: string | null;
}

export interface UsageLogStatItem {
  dimension: string;
  dimension_id: string | null;
  dimension_name: string;
  requests: number;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  cache_hit_tokens: number;
  cache_miss_tokens: number;
  avg_duration_ms: number;
  max_duration_ms: number;
}

export interface UsageLogStatsResponse {
  total: UsageLogStatItem;
  items: UsageLogStatItem[];
}
