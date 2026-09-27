import { useState } from "react";

type Props = { label: string };

export function Counter({ label }: Props) {
  const [count, setCount] = useState(0);
  return <button onClick={() => setCount(count + 1)}>{label}: {count}</button>;
}
