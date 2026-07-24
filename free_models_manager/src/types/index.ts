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
  provider_id: number;
  name: string;
  model_id: string;
  timeout: number;
  protocols: string;
  context_length: number;
  priority: number;
  status: 'available' | 'unavailable' | 'deprecated';
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

export type PageKey = 'overview' | 'providers' | 'models' | 'apiKeys' | 'settings';

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
