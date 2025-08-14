import React from 'react';

const Logo = ({ className = "" }: { className?: string }) => {
  return (
    <div className={`flex items-center justify-center gap-8 ${className}`}>
      {/* Lock Icon with Gradient */}
      <div className="relative">
        <svg
          width="64"
          height="64"
          viewBox="0 0 64 64"
          fill="none"
          xmlns="http://www.w3.org/2000/svg"
          className="drop-shadow-lg"
        >
          {/* Lock body */}
          <rect
            x="16"
            y="26"
            width="32"
            height="22"
            rx="4"
            fill="url(#lockGradientSimple)"
          />
          
          {/* Lock shackle */}
          <path
            d="M21 26V21C21 15.4 26.4 10 32 10C37.6 10 43 15.4 43 21V26"
            fill="none"
            stroke="url(#lockGradientSimple)"
            strokeWidth="5"
            strokeLinecap="round"
          />
          
          {/* Keyhole */}
          <circle
            cx="32"
            cy="35"
            r="2.5"
            fill="white"
          />
          <rect
            x="31"
            y="35"
            width="2"
            height="6"
            fill="white"
          />
          
          <defs>
            <linearGradient id="lockGradientSimple" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#8b5cf6" />
              <stop offset="50%" stopColor="#3b82f6" />
              <stop offset="100%" stopColor="#06b6d4" />
            </linearGradient>
          </defs>
        </svg>
      </div>
      
      {/* Vertical Separator */}
      <div className="w-0.5 h-16 bg-gradient-to-b from-purple-500 via-blue-500 to-cyan-500"></div>
      
      {/* Logo Text - EaZip seulement */}
      <div className="flex flex-col">
        <h1 className="text-5xl font-bold text-foreground tracking-wider font-sans" style={{
          textShadow: '2px 2px 0px rgba(0,0,0,0.1)'
        }}>
          EaZip
        </h1>
      </div>
    </div>
  )
}

export default Logo;