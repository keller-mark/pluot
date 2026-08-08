import React from "react";


const boxShadow = '5px 5px 15px rgb(0 0 0 / 20%)';

export function Tooltip(props) {
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
        <table style={{ borderCollapse: 'collapse', marginBottom: 0, opacity: 0.9, padding: '5px', backgroundColor: 'white', borderRadius: '2px', boxShadow }}>
          <tbody>
            {Object.entries(content).map(([key, value]) => (
              <tr key={key}>
                <th style={{ border: 'none', fontSize: '12px', outline: 0, padding: '0 2px', }}>{key}</th>
                <td style={{ border: 'none', fontSize: '12px', outline: 0, padding: '0 2px', }}>{value}</td>
              </tr>
            ))}
          </tbody>
        </table>
    );
  }
  return <pre>{JSON.stringify(content, null, 2)}</pre>;
}
