package sh.uze;

public final class Main {
    public static void main(String[] args) {
        var name = args.length > 0 ? args[0] : "world";
        System.out.println("Hello, " + name);
    }
}
