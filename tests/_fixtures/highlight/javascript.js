const items = [1, 2, 3];

function total(values) {
  return values.reduce((sum, value) => sum + value, 0);
}

console.log(`total: ${total(items)}`);
