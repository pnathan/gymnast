;;; Twitter clone: a classic distributed systems design problem.
;;;
;;; Core challenges: fan-out on write for home timelines, follower
;;; graph consistency, celebrity accounts with millions of followers,
;;; idempotent likes/retweets, cursor-based pagination over
;;; eventually-consistent timeline materialization, and rate limiting.

(defspec twitter-go-spec
  :version "0.1"
  :owner platform
  :exports (UserId TweetId TimelineId
    Tweet User Timeline FollowGraph
    tweet-service timeline-service social-service)

  (application chirp
    :modules (twitter-spec)
    :default-acceptance production)

  ;; --- Actors ---

  (actor user
    :kind person
    :identity (oauth2 issuer subject))

  (actor system
    :kind service
    :identity (internal service-account))

  ;; --- Core types ---

  (type UserId :opaque Text)
  (type TweetId :opaque Text)
  (type TimelineId :opaque Text)
  (type FollowId :opaque Text)
  (type NotificationId :opaque Text)
  (type Version :opaque Integer)
  (type Timestamp :opaque Integer)

  (type TweetKind :enum (original reply retweet quote))
  (type TimelineKind :enum (home user mentions))
  (type NotificationKind
    :enum (mention like retweet follow reply quote))

  (type MediaRef
    :record ((url Text)
      (media-type Text)
      (alt-text (Optional Text))))

  (type User
    :record ((id UserId)
      (handle (Text :min 1 :max 15))
      (display-name (Text :min 1 :max 50))
      (bio (Text :max 160))
      (follower-count Integer)
      (following-count Integer)
      (tweet-count Integer)
      (created-at Timestamp)
      (version Version)))

  (type Tweet
    :record ((id TweetId)
      (author UserId)
      (body (Text :min 1 :max 280))
      (kind TweetKind)
      (reply-to (Optional TweetId))
      (retweet-of (Optional TweetId))
      (quote-of (Optional TweetId))
      (media (List MediaRef))
      (like-count Integer)
      (retweet-count Integer)
      (reply-count Integer)
      (created-at Timestamp)
      (version Version)))

  (type FollowEdge
    :record ((id FollowId)
      (follower UserId)
      (followed UserId)
      (created-at Timestamp)))

  (type TimelineEntry
    :record ((tweet-id TweetId)
      (score Timestamp)
      (reason TweetKind)))

  (type Notification
    :record ((id NotificationId)
      (recipient UserId)
      (kind NotificationKind)
      (actor UserId)
      (subject-tweet (Optional TweetId))
      (created-at Timestamp)
      (read Boolean)))

  ;; --- Components ---

  (component tweet-engine
    :responsibility "Accept, store, and retrieve tweets"
    :provides (tweet-service)
    :uses (auth-provider durable-store clock id-source rate-limiter))

  (component timeline-engine
    :responsibility "Materialize and serve home/user/mention timelines"
    :provides (timeline-service)
    :uses (durable-store event-bus clock cache))

  (component social-engine
    :responsibility "Manage follow graph and notifications"
    :provides (social-service)
    :uses (auth-provider durable-store clock id-source))

  ;; --- Interfaces ---

  (interface tweet-service
    (command post-tweet
      :actor user
      :input (record (body Text) (kind TweetKind)
        (reply-to (Optional TweetId))
        (quote-of (Optional TweetId))
        (media (List MediaRef)))
      :output Tweet
      :errors (unauthenticated rate-limited validation-failed
        not-found blocked))
    (command delete-tweet
      :actor user
      :input (record (tweet-id TweetId))
      :output Void
      :errors (unauthenticated not-found forbidden))
    (command like-tweet
      :actor user
      :input (record (tweet-id TweetId))
      :output Void
      :errors (unauthenticated not-found already-liked))
    (command unlike-tweet
      :actor user
      :input (record (tweet-id TweetId))
      :output Void
      :errors (unauthenticated not-found not-liked))
    (query get-tweet
      :actor user
      :input (record (tweet-id TweetId))
      :output Tweet
      :errors (unauthenticated not-found))
    (query get-replies
      :actor user
      :input (record (tweet-id TweetId) (cursor (Optional Cursor)))
      :output (Page Tweet)
      :errors (unauthenticated not-found invalid-cursor)))

  (interface timeline-service
    (query home-timeline
      :actor user
      :input (record (cursor (Optional Cursor)) (limit (Optional Integer)))
      :output (Page TimelineEntry)
      :errors (unauthenticated invalid-cursor))
    (query user-timeline
      :actor user
      :input (record (user-id UserId) (cursor (Optional Cursor)))
      :output (Page Tweet)
      :errors (unauthenticated not-found invalid-cursor))
    (query mentions-timeline
      :actor user
      :input (record (cursor (Optional Cursor)))
      :output (Page Tweet)
      :errors (unauthenticated invalid-cursor)))

  (interface social-service
    (command follow
      :actor user
      :input (record (target UserId))
      :output FollowEdge
      :errors (unauthenticated not-found already-following
        blocked self-follow))
    (command unfollow
      :actor user
      :input (record (target UserId))
      :output Void
      :errors (unauthenticated not-found not-following))
    (query followers
      :actor user
      :input (record (user-id UserId) (cursor (Optional Cursor)))
      :output (Page User)
      :errors (unauthenticated not-found invalid-cursor))
    (query following
      :actor user
      :input (record (user-id UserId) (cursor (Optional Cursor)))
      :output (Page User)
      :errors (unauthenticated not-found invalid-cursor))
    (query notifications
      :actor user
      :input (record (cursor (Optional Cursor)))
      :output (Page Notification)
      :errors (unauthenticated invalid-cursor)))

  ;; --- State ---

  (state tweet-store
    :of (aggregate Tweet Like)
    :owner tweet-engine
    :durability durable
    :initial empty
    :aggregate (per-tweet TweetId)
    :versioned optimistic
    :partitioned-by author
    :consistency serial-per-tweet)

  (state follow-graph
    :of (aggregate FollowEdge)
    :owner social-engine
    :durability durable
    :initial empty
    :aggregate (per-user UserId)
    :versioned optimistic
    :partitioned-by follower
    :consistency serial-per-edge)

  (state user-profiles
    :of (aggregate User)
    :owner social-engine
    :durability durable
    :initial empty
    :aggregate (per-user UserId)
    :versioned optimistic
    :consistency serial-per-user)

  (state timeline-store
    :of (aggregate TimelineEntry)
    :owner timeline-engine
    :durability durable
    :initial empty
    :aggregate (per-user UserId)
    :versioned none
    :consistency eventual)

  (state notification-store
    :of (aggregate Notification)
    :owner social-engine
    :durability durable
    :initial empty
    :aggregate (per-user UserId)
    :versioned none
    :consistency eventual)

  ;; --- Flows ---

  (flow user-to-tweets
    :from user
    :to tweet-service
    :kind command
    :grant (authenticated-session)
    :deny (raw-oauth-token))

  (flow user-to-social
    :from user
    :to social-service
    :kind command
    :grant (authenticated-session)
    :deny (raw-oauth-token))

  (flow user-to-timeline
    :from user
    :to timeline-service
    :kind query
    :grant (authenticated-session)
    :deny (raw-oauth-token))

  (flow tweet-events-to-timeline
    :from tweet-engine
    :to timeline-engine
    :kind event
    :data (tweet-created tweet-deleted))

  (flow social-events-to-timeline
    :from social-engine
    :to timeline-engine
    :kind event
    :data (followed unfollowed))

  ;; --- Behaviors ---

  (behavior post-tweet
    :on (tweet-service/post-tweet user request)
    :reads (tweets user-profiles)
    :writes (tweets)
    :atomic (tweet request/body)
    :idempotency command-key
    (requires (authenticated? user))
    (requires (not (rate-exceeded? user 300 per-hour)))
    (requires (<= (length request/body) 280))
    (requires (if request/reply-to (exists? tweets request/reply-to) true))
    (requires (if request/reply-to
        (not (blocked-by? request/reply-to/author user)) true))
    (ensures (= post (insert-tweet pre request result)))
    (ensures (= result/author user/id))
    (returns result)
    (fails rate-limited
      :when (rate-exceeded? user 300 per-hour)
      :preserves all-state)
    (fails not-found
      :when (and request/reply-to (not (exists? tweets request/reply-to)))
      :preserves all-state)
    (fails blocked
      :when (and request/reply-to (blocked-by? request/reply-to/author user))
      :preserves all-state)
    (emits tweet-created :at-least-once))

  (behavior delete-tweet
    :on (tweet-service/delete-tweet user request)
    :reads (tweets)
    :writes (tweets)
    :atomic (tweet request/tweet-id)
    :idempotency command-key
    (requires (authenticated? user))
    (requires (exists? tweets request/tweet-id))
    (requires (= (tweet-author tweets request/tweet-id) user/id))
    (ensures (not (exists? post/tweets request/tweet-id)))
    (fails forbidden
      :when (not (= (tweet-author tweets request/tweet-id) user/id))
      :preserves all-state)
    (emits tweet-deleted :at-least-once))

  (behavior like-tweet
    :on (tweet-service/like-tweet user request)
    :reads (tweets likes)
    :writes (likes tweets)
    :atomic (like user/id request/tweet-id)
    :idempotency (user/id request/tweet-id)
    (requires (authenticated? user))
    (requires (exists? tweets request/tweet-id))
    (requires (not (liked? likes user/id request/tweet-id)))
    (ensures (liked? post/likes user/id request/tweet-id))
    (ensures (= post/tweet/like-count (+ pre/tweet/like-count 1)))
    (fails already-liked
      :when (liked? likes user/id request/tweet-id)
      :preserves all-state)
    (emits tweet-liked :at-least-once))

  (behavior follow-user
    :on (social-service/follow user request)
    :reads (follow-graph user-profiles)
    :writes (follow-graph user-profiles)
    :atomic (follow-edge user/id request/target)
    :idempotency (user/id request/target)
    (requires (authenticated? user))
    (requires (not (= user/id request/target)))
    (requires (exists? user-profiles request/target))
    (requires (not (following? follow-graph user/id request/target)))
    (requires (not (blocked-by? request/target user)))
    (ensures (following? post/follow-graph user/id request/target))
    (ensures (= post/target/follower-count (+ pre/target/follower-count 1)))
    (ensures (= post/user/following-count (+ pre/user/following-count 1)))
    (fails already-following
      :when (following? follow-graph user/id request/target)
      :preserves all-state)
    (fails self-follow
      :when (= user/id request/target)
      :preserves all-state)
    (emits followed :at-least-once))

  (behavior unfollow-user
    :on (social-service/unfollow user request)
    :reads (follow-graph user-profiles)
    :writes (follow-graph user-profiles)
    :atomic (follow-edge user/id request/target)
    :idempotency (user/id request/target)
    (requires (authenticated? user))
    (requires (following? follow-graph user/id request/target))
    (ensures (not (following? post/follow-graph user/id request/target)))
    (ensures (= post/target/follower-count (- pre/target/follower-count 1)))
    (ensures (= post/user/following-count (- pre/user/following-count 1)))
    (fails not-following
      :when (not (following? follow-graph user/id request/target))
      :preserves all-state)
    (emits unfollowed :at-least-once))

  ;; --- Invariants ---

  (invariant tweet-length
    :scope tweet-store
    :always (forall ((tweet Tweet))
      (<= (length tweet/body) 280)))

  (invariant no-self-follow
    :scope follow-graph
    :always (forall ((edge FollowEdge))
      (not (= edge/follower edge/followed))))

  (invariant follower-count-consistency
    :scope user-profiles
    :always (forall ((u User))
      (= u/follower-count
        (count-edges follow-graph :followed u/id))))

  (invariant following-count-consistency
    :scope user-profiles
    :always (forall ((u User))
      (= u/following-count
        (count-edges follow-graph :follower u/id))))

  (invariant like-count-consistency
    :scope tweet-store
    :always (forall ((tweet Tweet))
      (= tweet/like-count
        (count-likes likes tweet/id))))

  (invariant unique-follow-edges
    :scope follow-graph
    :always (forall ((a FollowEdge) (b FollowEdge))
      (implies (and (= a/follower b/follower)
          (= a/followed b/followed))
        (= a/id b/id))))

  (invariant timeline-consistency
    :scope timeline-store
    :always (forall ((entry TimelineEntry) (owner UserId))
      (implies (in-timeline? entry owner)
        (or (following? follow-graph owner entry/tweet/author)
          (= owner entry/tweet/author)))))

  ;; --- Constraints ---

  (constraint tweet-rate-limit
    :class workload
    :scope tweet-service
    :under (workload
      :virtual-users 10000
      :duration (minutes 30)
      :write-p95 (milliseconds 200)
      :read-p95 (milliseconds 100))
    :must (forall ((u User))
      (<= (tweets-per-hour u) 300)))

  (constraint timeline-latency
    :class latency
    :scope timeline-service
    :under (workload
      :virtual-users 100000
      :duration (minutes 30)
      :read-p95 (milliseconds 500))
    :must (and (= missed-tweets 0)
      (<= timeline-staleness (seconds 30))))

  (constraint fanout-consistency
    :class consistency
    :scope timeline-engine
    :under (workload
      :virtual-users 50000
      :duration (minutes 60))
    :must (and
      (eventually-consistent timeline-store (seconds 30))
      (= lost-tweets 0)
      (= phantom-tweets 0)))

  (constraint celebrity-fanout
    :class scalability
    :scope timeline-engine
    :under (workload
      :celebrity-followers 10000000
      :tweet-rate 1-per-minute)
    :must (and
      (<= fanout-latency-p99 (seconds 60))
      (= lost-followers 0)))

  ;; --- Synthesis ---

  (synthesis prototype
    :target (go :framework stdlib)
    :platform gymnast-reference-platform-v1
    :model (small-code-model
      :class nano
      :temperature 0
      :max-attempts 3)
    :attempts 3
    :must-not (invent-product-semantics add-unpinned-dependencies))

  ;; --- Acceptance ---

  (acceptance production
    :subject chirp

    (property post-then-read
      :generate ((actor authenticated-user) (tweet valid-tweet))
      :execute (sequence (post-tweet actor tweet)
        (get-tweet actor result/id))
      :must (equivalent? result tweet))

    (property delete-removes
      :generate ((actor authenticated-user) (tweet valid-tweet))
      :execute (sequence (post-tweet actor tweet)
        (delete-tweet actor result/id)
        (get-tweet actor result/id))
      :must (fails-with not-found))

    (property like-idempotent
      :generate ((actor authenticated-user) (tweet existing-tweet))
      :execute (sequence (like-tweet actor tweet/id)
        (like-tweet actor tweet/id))
      :must (fails-with already-liked))

    (property follow-then-timeline
      :generate ((follower authenticated-user)
        (author authenticated-user)
        (tweet valid-tweet))
      :execute (sequence (follow follower author/id)
        (post-tweet author tweet)
        (wait (seconds 30))
        (home-timeline follower))
      :must (contains? result tweet))

    (property unfollow-removes-from-timeline
      :generate ((follower authenticated-user)
        (author authenticated-user)
        (tweet valid-tweet))
      :execute (sequence (follow follower author/id)
        (post-tweet author tweet)
        (wait (seconds 30))
        (unfollow follower author/id)
        (home-timeline follower))
      :must (not (contains? result tweet)))

    (property cannot-follow-self
      :generate ((actor authenticated-user))
      :execute (follow actor actor/id)
      :must (fails-with self-follow))

    (scenario celebrity-timeline
      (given celebrity (user-with-followers 10000000))
      (given follower (authenticated-user))
      (when (follow follower celebrity/id))
      (then succeeds)
      (when (post-tweet celebrity (valid-tweet)))
      (then succeeds)
      (when (wait (seconds 60)))
      (when (home-timeline follower))
      (then (contains? result tweet)))

    (scenario rate-limit-enforcement
      (given actor (authenticated-user))
      (when (repeat 300 (post-tweet actor (valid-tweet))))
      (then succeeds)
      (when (post-tweet actor (valid-tweet)))
      (then (fails-with rate-limited)))

    (scenario reply-thread
      (given author (authenticated-user))
      (given replier (authenticated-user))
      (when (post-tweet author (valid-tweet)))
      (then succeeds)
      (when (post-tweet replier (reply-to result)))
      (then succeeds)
      (when (get-replies author tweet/id))
      (then (contains? result reply)))

    (concurrency like-storm
      :actors 10000
      :schedule adversarial
      :must (= tweet/like-count (count-unique-likers)))

    (concurrency follow-race
      :actors 1000
      :schedule adversarial
      :must (and (= lost-follows 0)
        (= u/follower-count (count-edges follow-graph :followed u/id))))

    (concurrency timeline-fanout-race
      :actors 50000
      :schedule adversarial
      :must (and (= lost-tweets 0)
        (= phantom-tweets 0)))

    (fault tweet-restart-durability
      :after acknowledged-write
      :inject restart
      :must (read-your-acknowledged-write))

    (fault timeline-crash-recovery
      :after tweet-created-event
      :inject restart
      :must (eventually-delivered timeline-store (seconds 60)))

    (fault follower-count-partition
      :after follow-edge-written
      :inject network-partition
      :must (eventually-consistent follower-count (seconds 120)))

    (fault duplicate-event-delivery
      :after tweet-created-event
      :inject duplicate-delivery
      :must (= timeline-entry-count 1))

    (coverage
      :every-operation true
      :every-error true
      :every-transition true
      :every-invariant true
      :boundaries true)

    (execution
      :clock virtual
      :randomness seeded
      :network controlled
      :timezone "UTC")))
