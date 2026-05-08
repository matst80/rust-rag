import React from 'react';

const DEFAULT_TABS = ['search', 'chat', 'store'] as const;

export interface TabBarProps {
  active: string;
  onChange?: (t: string) => void;
  tabs?: readonly string[];
}

export function TabBar({ active, onChange, tabs = DEFAULT_TABS }: TabBarProps) {
  const idx = tabs.indexOf(active);

  return (
    <div className="tab-bar">
      {tabs.map((tab) => (
        <button
          key={tab}
          className={`tab-btn${active === tab ? ' active' : ''}`}
          onClick={() => onChange?.(tab)}
        >
          {tab}
        </button>
      ))}

      <div
        className="tab-indicator"
        style={{ left: `${(idx / tabs.length) * 100}%`, width: `${100 / tabs.length}%` }}
      />
    </div>
  );
}
