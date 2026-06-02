import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import type { FormEvent } from 'react';
import {
  createInstanceGrant,
  deleteSpace,
  getMe,
  listAllInstanceGrants,
  listAllSpaces,
  listUsers,
  revokeInstanceGrant
} from '$lib/api/admin';
import type { InstanceAccess } from '$lib/types';
import { Badge } from '../../ui/Badge';
import { EmptyState } from '../../ui/EmptyState';
import { Panel } from '../../ui/Panel';
import { Spinner } from '../../ui/Spinner';
import { Table } from '../../ui/Table';
import './admin.css';

// Render a single labelled summary statistic.
function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="admin-stat">
      <span className="admin-stat__value">{value}</span>
      <span className="admin-stat__label">{label}</span>
    </div>
  );
}

// Render the admin Spaces and Sharing console: a team-wide view of delegated
// instance access (whole-instance, sharded sharing model) with grant and
// revoke controls, plus an overview of every named space.
export function Spaces() {
  const queryClient = useQueryClient();
  const me = useQuery({ queryFn: getMe, queryKey: ['me'] });
  const isAdmin = me.data?.is_admin === true;

  const users = useQuery({ enabled: isAdmin, queryFn: listUsers, queryKey: ['admin', 'users'] });
  const grants = useQuery({
    enabled: isAdmin,
    queryFn: listAllInstanceGrants,
    queryKey: ['admin', 'all-grants']
  });
  const spaces = useQuery({ enabled: isAdmin, queryFn: listAllSpaces, queryKey: ['admin', 'all-spaces'] });

  const [ownerId, setOwnerId] = useState<number | null>(null);
  const [granteeId, setGranteeId] = useState<number | null>(null);
  const [access, setAccess] = useState<InstanceAccess>('read');
  const [error, setError] = useState<string | null>(null);

  // Create the grant described by the form, then refresh the shares overview.
  async function handleGrant(event: FormEvent) {
    event.preventDefault();
    setError(null);
    if (ownerId == null || granteeId == null) {
      return;
    }
    try {
      await createInstanceGrant({ access, grantee_user_id: granteeId, owner_user_id: ownerId });
      setGranteeId(null);
      await queryClient.invalidateQueries({ queryKey: ['admin', 'all-grants'] });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Grant failed');
    }
  }

  // Revoke a grantee's access to an owner's instance.
  async function handleRevoke(owner: number, grantee: number) {
    setError(null);
    try {
      await revokeInstanceGrant(owner, grantee);
      await queryClient.invalidateQueries({ queryKey: ['admin', 'all-grants'] });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Revoke failed');
    }
  }

  // Delete a named space, then refresh the spaces overview.
  async function handleDeleteSpace(id: number, name: string) {
    setError(null);
    if (!window.confirm(`Delete the space "${name}"? Memories keep their data but lose this space.`)) {
      return;
    }
    try {
      await deleteSpace(id);
      await queryClient.invalidateQueries({ queryKey: ['admin', 'all-spaces'] });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Delete failed');
    }
  }

  if (me.isLoading) {
    return <Spinner />;
  }

  if (!isAdmin) {
    return (
      <div>
        <header className="route-header">
          <div>
            <h1>Spaces &amp; Sharing</h1>
            <p>Delegated instance access</p>
          </div>
        </header>
        <Panel>
          <EmptyState
            message="You need the admin scope to manage instance access grants."
            title="Admins only"
          />
        </Panel>
      </div>
    );
  }

  const userList = users.data ?? [];
  const grantRows = grants.data ?? [];
  const spaceRows = spaces.data ?? [];
  const granteeOptions = userList.filter((u) => u.id !== ownerId);

  return (
    <div>
      <header className="route-header">
        <div>
          <h1>Spaces &amp; Sharing</h1>
          <p>Delegated read or write access to a user&apos;s entire instance, across the team</p>
        </div>
      </header>

      <div className="admin-stats">
        <Stat label="Active shares" value={grantRows.length} />
        <Stat label="Named spaces" value={spaceRows.length} />
        <Stat label="Users" value={userList.length} />
      </div>

      <div className="admin-stack">
        <Panel title="Grant instance access">
          <form className="admin-grant-form" onSubmit={handleGrant}>
            <label className="admin-field">
              <span>Owner</span>
              <select
                onChange={(event) => {
                  setOwnerId(event.target.value ? Number(event.target.value) : null);
                  setGranteeId(null);
                }}
                value={ownerId ?? ''}
              >
                <option value="">Select a user...</option>
                {userList.map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.username}
                  </option>
                ))}
              </select>
            </label>
            <label className="admin-field">
              <span>Grantee</span>
              <select
                disabled={ownerId == null}
                onChange={(event) => setGranteeId(event.target.value ? Number(event.target.value) : null)}
                value={granteeId ?? ''}
              >
                <option value="">Select a user...</option>
                {granteeOptions.map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.username}
                  </option>
                ))}
              </select>
            </label>
            <label className="admin-field">
              <span>Access</span>
              <select onChange={(event) => setAccess(event.target.value as InstanceAccess)} value={access}>
                <option value="read">Read</option>
                <option value="write">Write</option>
              </select>
            </label>
            <button className="admin-grant-button" disabled={ownerId == null || granteeId == null} type="submit">
              Grant
            </button>
          </form>
          {error ? (
            <p className="admin-error" role="alert">
              {error}
            </p>
          ) : null}
        </Panel>

        <Panel title="All shares">
          {grants.isLoading ? (
            <Spinner />
          ) : grantRows.length > 0 ? (
            <Table
              headers={['Owner', 'Grantee', 'Access', 'Granted by', 'Created', '']}
              rows={grantRows.map((g) => [
                g.owner_username ?? `#${g.owner_user_id}`,
                g.grantee_username ?? `#${g.grantee_user_id}`,
                <Badge key="a" label={g.access} tone={g.access === 'write' ? 'warn' : 'ok'} />,
                g.granted_by_username ?? `#${g.granted_by}`,
                g.created_at ? g.created_at.slice(0, 19) : '--',
                <button
                  className="admin-revoke"
                  key="r"
                  onClick={() => handleRevoke(g.owner_user_id, g.grantee_user_id)}
                  type="button"
                >
                  Revoke
                </button>
              ])}
            />
          ) : (
            <EmptyState
              hint="Use the form above to grant read or write access."
              message="No user has delegated access to another's instance yet."
              title="No shares yet"
            />
          )}
        </Panel>

        <Panel title="Spaces">
          {spaces.isLoading ? (
            <Spinner />
          ) : spaceRows.length > 0 ? (
            <Table
              headers={['Owner', 'Space', 'Description', 'Created', '']}
              rows={spaceRows.map((s) => [
                s.owner_username ?? `#${s.owner_user_id}`,
                s.name,
                s.description ?? '--',
                s.created_at ? s.created_at.slice(0, 19) : '--',
                s.name === 'default' ? (
                  ''
                ) : (
                  <button
                    className="admin-delete"
                    key="d"
                    onClick={() => handleDeleteSpace(s.id, s.name)}
                    type="button"
                  >
                    Delete
                  </button>
                )
              ])}
            />
          ) : (
            <EmptyState message="No named spaces exist yet." title="No spaces" />
          )}
        </Panel>
      </div>
    </div>
  );
}
