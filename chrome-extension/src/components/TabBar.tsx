import type { Tab } from '../types';

const TABS: Tab[] = ['search', 'chat', 'store'];

interface Props {
  active: Tab;
  onChange: (t: Tab) => void;
}

export function TabBar({ active, onChange }: Props) {
  const idx = TABS.indexOf(active);

  return (
    <div className="tab-bar">
      {TABS.map((tab) => (
        <button
          key={tab}
          className={`tab-btn${active === tab ? ' active' : ''}`}
          onClick={() => onChange(tab)}
        >
          {tab}
        </button>
      ))}
      <div
        className="tab-indicator"
        style={{
          left: `${(idx / TABS.length) * 100}%`,
          width: `${100 / TABS.length}%`,
        }}
      />
    </div>
  );
}
