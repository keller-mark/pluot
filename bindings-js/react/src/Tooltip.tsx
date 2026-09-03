import React, { type ReactNode } from "react";
import type { TooltipProps } from "./types.js";


const boxShadow = '5px 5px 15px rgb(0 0 0 / 20%)';

// Table cell values come from arbitrary onHover return objects, so narrow
// them to something React can actually render.
function renderCell(value: unknown): ReactNode {
  if (value === null || value === undefined || typeof value === "boolean") {
    return null;
  }
  if (typeof value === "string" || typeof value === "number") {
    return value;
  }
  if (React.isValidElement(value)) {
    return value;
  }
  return JSON.stringify(value);
}

export function Tooltip(props: TooltipProps) {
  const {
    content,
    asTable = false,
  } = props;
  if (!content) {
    return null;
  }
  if (typeof content === "string" || typeof content === "number") {
    return <pre>{content}</pre>;
  }
  if (React.isValidElement(content)) {
    return content;
  }
  if (asTable) {
    return (
        <table style={{ display: 'inline-block', marginBottom: 0, opacity: 0.9, padding: '5px', backgroundColor: 'white', borderRadius: '2px', boxShadow }}>
          <tbody>
            {Object.entries(content).map(([key, value]) => (
              <tr key={key}>
                <th style={{ border: 'none', fontSize: '12px', outline: 0, padding: '0 2px', textAlign: 'left' }}>{key}</th>
                <td style={{ border: 'none', fontSize: '12px', outline: 0, padding: '0 2px', textAlign: 'left' }}>{renderCell(value)}</td>
              </tr>
            ))}
          </tbody>
        </table>
    );
  }
  return <pre>{JSON.stringify(content, null, 2)}</pre>;
}
