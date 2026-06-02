import type { ReactNode } from 'react';

// Per-column horizontal alignment; 'right' is for numeric columns.
type Align = 'left' | 'right';

// Render a dense operational data table with tabular numerics and zebra rows.
export function Table({
  headers,
  rows,
  align
}: {
  headers: string[];
  rows: ReactNode[][];
  align?: Align[];
}) {
  // Resolve the cell class for a column, marking numeric columns for right-align.
  const colClass = (index: number) => (align?.[index] === 'right' ? 'num' : undefined);

  return (
    <table className="k-table">
      <thead>
        <tr>
          {headers.map((header, index) => (
            <th className={colClass(index)} key={header}>
              {header}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((row, rowIndex) => (
          <tr key={rowIndex}>
            {row.map((cell, cellIndex) => (
              <td className={colClass(cellIndex)} key={cellIndex}>
                {cell}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
