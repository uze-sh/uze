object Main extends App {
  case class User(id: Int, name: String)
  val users = List(User(1, "ada"), User(2, "linus"))
  users.map(_.name).foreach(println)
}
