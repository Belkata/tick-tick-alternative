# JNA looks up its own classes and the UniFFI interfaces by reflection.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class dev.tickalt.core.** { *; }
-dontwarn java.awt.**

# Glance uses WorkManager, whose Room database is created by reflection
# (RoomDatabase looks up "<Name>_Impl"). Without this the release build crashes on start.
-keep class * extends androidx.room.RoomDatabase { <init>(); }
-keep class androidx.work.impl.WorkDatabase_Impl { *; }
