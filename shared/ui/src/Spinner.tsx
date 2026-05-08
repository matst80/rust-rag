import React from 'react';

export function Spinner({ size = 16, className = '' }: { size?: number; className?: string }) {
  const style: React.CSSProperties = { width: size, height: size };
  return <div className={`spinner ${className}`} style={style} />;
}
