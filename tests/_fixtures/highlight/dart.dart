class User {
  final int id;
  final String name;
  const User(this.id, this.name);
}

void main() {
  print(User(1, 'ada').name);
}
