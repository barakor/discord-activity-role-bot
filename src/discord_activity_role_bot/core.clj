(ns discord-activity-role-bot.core
  (:require 
            [discord-activity-role-bot.handle-presence :refer [presence-update]]
            [discord-activity-role-bot.handle-db :refer [load-db!]]
            [clojure.core.async :as async :refer [close!]]
            
            [discljord.messaging :as discord-rest :refer [start-connection! stop-connection! get-current-user!]]  
            [discljord.connections :as discord-ws]
            [discljord.events :refer [message-pump!]]
            [discljord.events.state :as discord-state :refer [caching-transducer]]                                                        

            [com.rpl.specter :as s]
            
            [taoensso.timbre :as timbre :refer [log]]
            [taoensso.timbre.tools.logging :refer [use-timbre]]

            [discord-activity-role-bot.state :refer [state* discord-state* config]]
            [discord-activity-role-bot.lazy-null :refer [easter]]))
     
(use-timbre)


(def bot-id (atom nil))



(defmulti handle-event (fn [type _data] type))


(defmethod handle-event :default [event-type event-data])
  ; (log :report (str "event type: " event-type "\nevent-data: " event-data)))


(defmethod handle-event :ready
  [_ event-data]
  (let [guild-ids (s/select [:guilds s/ALL :id] event-data)]
    (log :info (str "logged in to guilds: " guild-ids))
    (log :info (str "discord-state cache " (when-not @discord-state* "not ") "available"))
    (discord-ws/status-update! (:gateway @state*) :activity (discord-ws/create-activity :name (:playing config)))
    (easter guild-ids)))


(defmethod handle-event :presence-update
  [_ event-data]
  (let [rest-connection (:rest @state*)] 
    (presence-update event-data rest-connection discord-state*)))


(defn start-bot! [token & {:keys [intents]}]
  (let [caching (caching-transducer discord-state* discord-state/caching-handlers)
        event-channel (async/chan (async/sliding-buffer 100000) caching)
        gateway-connection (discord-ws/connect-bot! token event-channel :intents intents)
        rest-connection (start-connection! token)]
    {:events  event-channel
     :gateway gateway-connection
     :rest    rest-connection
     :config  config}))


(defn stop-bot! [{:keys [rest gateway events] :as _state*}]
  (stop-connection! rest)
  (discord-ws/disconnect-bot! gateway)
  (close! events))

(defn -main [& args]
  (reset! state* (start-bot! (:token config) :intents (:intents config)))
  (reset! bot-id (:id @(get-current-user! (:rest @state*))))
  (load-db!)
  (try
    (message-pump! (:events @state*) handle-event)
    (catch Exception e (log :error (str "Exception at -main level, maybe I can handle it here? " e)))
    (finally (stop-bot! @state*))))


(comment
  (-main))
  


