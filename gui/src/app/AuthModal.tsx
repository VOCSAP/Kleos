import { useState } from 'react';
import type { FormEvent } from 'react';

// Render the modal used to save a bearer API key for same-origin API calls.
export function AuthModal({
  onClose,
  onSave,
  open
}: {
  onClose: () => void;
  onSave: (value: string) => void;
  open: boolean;
}) {
  const [value, setValue] = useState('');

  if (!open) {
    return null;
  }

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const trimmed = value.trim();
    if (trimmed) {
      onSave(trimmed);
      setValue('');
    }
  };

  const close = () => {
    setValue('');
    onClose();
  };

  return (
    <div aria-labelledby="auth-modal-title" aria-modal="true" className="auth-modal" role="dialog">
      <form className="auth-modal__panel" onSubmit={submit}>
        <h2 id="auth-modal-title">API Key</h2>
        <label className="auth-modal__label" htmlFor="kleos-api-key">
          API key
        </label>
        <input
          autoFocus
          id="kleos-api-key"
          onChange={(event) => setValue(event.target.value)}
          type="password"
          value={value}
        />
        <div className="auth-modal__actions">
          <button className="auth-modal__save" type="submit">
            Save
          </button>
          <button className="auth-modal__cancel" onClick={close} type="button">
            Cancel
          </button>
        </div>
      </form>
    </div>
  );
}
