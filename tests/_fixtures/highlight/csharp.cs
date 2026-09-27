using System;

namespace Uze;

public record User(int Id, string Name);

public static class Program
{
    public static void Main() => Console.WriteLine(new User(1, "ada"));
}
