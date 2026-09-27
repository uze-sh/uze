data class User(val id: Int, val name: String)

fun main() {
    val users = listOf(User(1, "ada"), User(2, "linus"))
    users.filter { it.id > 1 }.forEach { println(it.name) }
}
