import { request } from '$lib/http';
import type { AdminGrant, AdminSpace, InstanceAccess, InstanceGrant, KleosUser, Me } from '$lib/types';

// Fetch the authenticated caller's identity and scopes.
export const getMe = () => request<Me>('/me');

// List all users (admin scope required) for owner and grantee pickers.
export async function listUsers(): Promise<KleosUser[]> {
  return (await request<{ users: KleosUser[] }>('/users')).users ?? [];
}

// List every instance grant across all owners (admin overview).
export async function listAllInstanceGrants(): Promise<AdminGrant[]> {
  return (await request<{ grants: AdminGrant[] }>('/sharing/grants')).grants ?? [];
}

// List every named space across all users (admin overview).
export async function listAllSpaces(): Promise<AdminSpace[]> {
  return (await request<{ spaces: AdminSpace[] }>('/sharing/spaces')).spaces ?? [];
}

// Delete a named space by id (admin or the space owner).
export const deleteSpace = (id: number) =>
  request<{ deleted: boolean }>(`/spaces/${id}`, { method: 'DELETE' });

// List the instance grants an owner has issued (owner or admin).
export async function listInstanceGrants(ownerUserId: number): Promise<InstanceGrant[]> {
  return (await request<{ grants: InstanceGrant[] }>(`/instance-grants?owner=${ownerUserId}`)).grants ?? [];
}

// Create or update a grant delegating access to an owner's instance.
export const createInstanceGrant = (body: {
  owner_user_id: number;
  grantee_user_id: number;
  access: InstanceAccess;
}) => request<InstanceGrant>('/instance-grants', { body, method: 'POST' });

// Revoke a grant from an owner's instance.
export const revokeInstanceGrant = (ownerUserId: number, granteeUserId: number) =>
  request<{ revoked: boolean }>(`/instance-grants/${ownerUserId}/${granteeUserId}`, { method: 'DELETE' });
