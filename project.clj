(defproject discord-activity-role-bot "0.1.0-SNAPSHOT"
  :description "FIXME: write description"
  :url "http://example.com/FIXME"
  :license {:name "EPL-2.0 OR GPL-2.0-or-later WITH Classpath-exception-2.0"
            :url "https://www.eclipse.org/legal/epl-2.0/"}
  :dependencies [[org.clojure/clojure "1.11.1"]
                 [com.github.discljord/discljord "1.3.1"]
                 [com.github.johnnyjayjay/slash "0.6.0-SNAPSHOT"]
                 [cheshire "5.11.0"]
                 [com.rpl/specter "1.1.4"]
                 [philoskim/debux "0.9.0"]]
  :repl-options {:init-ns discord-activity-role-bot.core}
  :main discord-activity-role-bot.core)
