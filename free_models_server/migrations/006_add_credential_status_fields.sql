ALTER TABLE provider_credential
    ADD COLUMN quota_exhausted BOOLEAN NOT NULL DEFAULT FALSE;
