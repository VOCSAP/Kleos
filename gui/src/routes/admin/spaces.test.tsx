import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as adminApi from '$lib/api/admin';
import { Spaces } from './Spaces';

vi.mock('$lib/api/admin');

// Render the Spaces page inside a fresh, retry-free query client.
function renderSpaces() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <Spaces />
    </QueryClientProvider>
  );
}

describe('Spaces admin page', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it('shows grant management for admins', async () => {
    vi.mocked(adminApi.getMe).mockResolvedValue({
      is_admin: true,
      scopes: ['admin'],
      user_id: 1,
      username: 'root'
    });
    vi.mocked(adminApi.listUsers).mockResolvedValue([
      { id: 2, username: 'alice' },
      { id: 3, username: 'bob' }
    ]);
    vi.mocked(adminApi.listAllInstanceGrants).mockResolvedValue([
      {
        access: 'read',
        created_at: '2026-05-31 00:00:00',
        granted_by: 1,
        granted_by_username: 'root',
        grantee_user_id: 3,
        grantee_username: 'bob',
        owner_user_id: 2,
        owner_username: 'alice'
      }
    ]);
    vi.mocked(adminApi.listAllSpaces).mockResolvedValue([]);

    renderSpaces();

    expect(await screen.findByRole('heading', { name: 'Spaces & Sharing' })).toBeInTheDocument();
    // The all-shares overview panel renders.
    expect(await screen.findByText('All shares')).toBeInTheDocument();
    // The grant row resolves the granted-by username, which appears only in the
    // overview table (not in the owner/grantee pickers).
    expect(await screen.findByText('root')).toBeInTheDocument();
    // The owner picker is populated from the user list.
    expect(screen.getAllByRole('option', { name: 'alice' }).length).toBeGreaterThan(0);
  });

  it('blocks non-admins', async () => {
    vi.mocked(adminApi.getMe).mockResolvedValue({
      is_admin: false,
      scopes: ['read', 'write'],
      user_id: 5,
      username: 'alice'
    });

    renderSpaces();

    expect(await screen.findByText('Admins only')).toBeInTheDocument();
  });
});
